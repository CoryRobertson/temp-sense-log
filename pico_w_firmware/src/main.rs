#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]

use core::future::Future;
use core::net::Ipv4Addr;
use core::num;
use core::str::FromStr;
use cyw43_pio::PioSpi;
use defmt::*;
use embassy_executor::Spawner;
use embassy_net::dns::DnsSocket;
use embassy_net::tcp::client::{TcpClient, TcpClientState};
use embassy_net::{Ipv4Address, Ipv4Cidr, Stack, StackResources};
use embassy_rp::clocks::RoscRng;
use embassy_rp::peripherals::DMA_CH0;
use embassy_rp::pio::Pio;
use embassy_rp::{
    bind_interrupts, gpio,
    i2c::{self, I2c, InterruptHandler},
    peripherals::{I2C0, PIO0},
    Peripheral,
};
use embassy_time::{Duration, Ticker, Timer};
use gpio::{Level, Output};
use heapless::Vec;
use lexical_core::write_float_options::Options;
use rand_core::RngCore;
use reqwless::client::HttpClient;
use reqwless::request::Method;
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct IrqsI2C {
    I2C0_IRQ => InterruptHandler<I2C0>;
});

bind_interrupts!(struct IrqsWifi {
    PIO0_IRQ_0 => embassy_rp::pio::InterruptHandler<PIO0>;
});

pub static WIFI_NETWORK: &str = env!("WIFI_NETWORK_PICO");
pub static WIFI_PASSWORD: &str = env!("WIFI_PASSWORD_PICO");
pub static READING_PERIOD: Option<&str> = option_env!("READING_PERIOD");
pub static BASE_URL: &str = env!("BASE_URL");
pub const FORMAT: u128 = lexical_core::format::STANDARD;

#[embassy_executor::task]
async fn cyw43_task(
    runner: cyw43::Runner<'static, Output<'static>, PioSpi<'static, PIO0, 0, DMA_CH0>>,
) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, cyw43::NetDriver<'static>>) -> ! {
    runner.run().await
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    // To make flashing faster for development, you may want to flash the firmwares independently
    // at hardcoded addresses, instead of baking them into the program with `include_bytes!`:
    //     probe-rs download cyw43-firmware/43439A0.bin --binary-format bin --chip RP2040 --base-address 0x10100000
    //     probe-rs download cyw43-firmware/43439A0_clm.bin --binary-format bin --chip RP2040 --base-address 0x10140000
    let fw = unsafe { core::slice::from_raw_parts(0x10100000 as *const u8, 230321) };
    let clm = unsafe { core::slice::from_raw_parts(0x10140000 as *const u8, 4752) };

    // networking pins
    let pwr = Output::new(p.PIN_23, Level::Low);
    let cs = Output::new(p.PIN_25, Level::High);
    let mut pio = Pio::new(p.PIO0, IrqsWifi);
    let spi = PioSpi::new(
        &mut pio.common,
        pio.sm0,
        pio.irq0,
        cs,
        p.PIN_24,
        p.PIN_29,
        p.DMA_CH0,
    );

    // network state objects and tasks
    static STATE: StaticCell<cyw43::State> = StaticCell::new();
    let state = STATE.init(cyw43::State::new());
    let (net_device, mut control, runner) = cyw43::new(state, pwr, spi, fw).await;
    unwrap!(spawner.spawn(cyw43_task(runner)));

    control.init(clm).await;
    control
        .set_power_management(cyw43::PowerManagementMode::PowerSave)
        .await;

    // let config = embassy_net::Config::dhcpv4(Default::default());
    // Use static IP configuration instead of DHCP
    let config = embassy_net::Config::ipv4_static(embassy_net::StaticConfigV4 {
        address: Ipv4Cidr::new(Ipv4Address::new(10, 0, 0, 225), 24),
        dns_servers: Vec::from_slice(&[
            Ipv4Addr::from_str("1.1.1.1").unwrap(),
            Ipv4Addr::from_str("8.8.8.8").unwrap(),
        ])
        .unwrap(),
        gateway: Some(Ipv4Address::new(10, 0, 0, 1)),
    });

    // network stack seed
    let seed = RoscRng::next_u32(&mut RoscRng);

    // network stack
    static RESOURCES: StaticCell<StackResources<5>> = StaticCell::new();
    let (stack, runner) = embassy_net::new(
        net_device,
        config,
        RESOURCES.init(StackResources::new()),
        seed as u64,
    );

    unwrap!(spawner.spawn(net_task(runner)));

    loop {
        match control.join_wpa2(WIFI_NETWORK, WIFI_PASSWORD).await {
            Ok(_) => break,
            Err(err) => {
                info!("join failed with status={}", err.status);
            }
        }
    }

    info!("waiting for DHCP...");
    while !stack.is_config_up() {
        Timer::after_millis(100).await;
    }
    info!("DHCP is now up!");

    info!("waiting for link up...");
    while !stack.is_link_up() {
        Timer::after_millis(500).await;
    }
    info!("Link is up!");

    info!("waiting for stack to be up...");
    stack.wait_config_up().await;
    info!("Stack is up!");

    info!("Base url: {}", BASE_URL);

    let sensor_sht = SHT40::new(unsafe {
        I2c::new_async(
            p.I2C0.clone_unchecked(),
            p.PIN_5.clone_unchecked(),
            p.PIN_4.clone_unchecked(),
            IrqsI2C,
            i2c::Config::default(),
        )
    });

    let sensor = AHT20::new(I2c::new_async(
        p.I2C0,
        p.PIN_5,
        p.PIN_4,
        IrqsI2C,
        i2c::Config::default(),
    ))
    .await;

    let options = lexical_core::WriteFloatOptions::builder()
        .inf_string(Some(b"Infinity"))
        .nan_string(Some(b"NaN"))
        .max_significant_digits(num::NonZeroUsize::new(4))
        .trim_floats(true)
        .build()
        .unwrap();

    let ticker = Ticker::every(Duration::from_secs(
        READING_PERIOD.unwrap_or("60").parse().unwrap(),
    ));

    // spawn the task that reads from the sensor, and then pushes that data to the web server
    match sensor {
        Ok(sensor) => {
            info!("Found AHT20 sensor");
            unwrap!(spawner.spawn(process_readings_aht(sensor, stack, options, ticker)));
        }
        Err(_) => match sensor_sht.await {
            Ok(sensor) => {
                info!("Found SHT40 sensor");
                unwrap!(spawner.spawn(process_readings_sht(sensor, stack, options, ticker)));
            }
            Err(err) => {
                error!("failed to find sensor: {}", err);
                defmt::panic!("Unable to find a sensor to use, panicking...");
            }
        },
    }
}

pub struct SHT40<'a> {
    i2c: I2c<'a, I2C0, i2c::Async>,
    mode: SHT40Mode,
}

pub enum SHT40Mode {
    NoHeatHighPrecision,
    NoHeatMedPrecision,
    NoHeatLowPrecision,
    HighHeat1s,
    HighHeat100ms,
    MedHeat1s,
    MedHeat100ms,
    LowHeat1s,
    LowHeat100ms,
}

impl SHT40Mode {
    pub const fn to_byte(&self) -> u8 {
        match self {
            SHT40Mode::NoHeatHighPrecision => 0xfd,
            SHT40Mode::NoHeatMedPrecision => 0xf6,
            SHT40Mode::NoHeatLowPrecision => 0xe0,
            SHT40Mode::HighHeat1s => 0x39,
            SHT40Mode::HighHeat100ms => 0x32,
            SHT40Mode::MedHeat1s => 0x2f,
            SHT40Mode::MedHeat100ms => 0x24,
            SHT40Mode::LowHeat1s => 0x1e,
            SHT40Mode::LowHeat100ms => 0x15,
        }
    }

    pub const fn get_delay(&self) -> Duration {
        match self {
            SHT40Mode::NoHeatHighPrecision => Duration::from_millis(10),
            SHT40Mode::NoHeatMedPrecision => Duration::from_millis(5),
            SHT40Mode::NoHeatLowPrecision => Duration::from_millis(2),
            SHT40Mode::MedHeat1s | SHT40Mode::HighHeat1s | SHT40Mode::LowHeat1s => {
                Duration::from_millis(1100)
            }
            SHT40Mode::MedHeat100ms | SHT40Mode::LowHeat100ms | SHT40Mode::HighHeat100ms => {
                Duration::from_millis(110)
            }
        }
    }
}

pub trait TempHumidSensor {
    fn get_reading(&mut self) -> impl Future<Output = Reading> + Send;
}

impl TempHumidSensor for SHT40<'_> {
    async fn get_reading(&mut self) -> Reading {
        self.measurement().await
    }
}

impl TempHumidSensor for AHT20<'_> {
    async fn get_reading(&mut self) -> Reading {
        self.get_reading().await
    }
}

impl<'a> SHT40<'a> {
    const SHT4X_DEFAULT_ADDR: u8 = 0x44;
    const SHT4X_READSERIAL: u8 = 0x89;
    const SHT4X_SOFTRESET: u8 = 0x94;

    pub async fn new(i2c: I2c<'a, I2C0, i2c::Async>) -> Result<Self, i2c::Error> {
        let mut sensor = Self {
            i2c,
            mode: SHT40Mode::NoHeatHighPrecision,
        };

        sensor.reset().await?;

        Ok(sensor)
    }

    pub fn set_mode(&mut self, mode: SHT40Mode) {
        self.mode = mode;
    }

    pub async fn reset(&mut self) -> Result<(), i2c::Error> {
        self.i2c
            .write_async(Self::SHT4X_DEFAULT_ADDR, [Self::SHT4X_SOFTRESET])
            .await?;
        Timer::after_millis(1).await;

        Ok(())
    }

    pub async fn measurement(&mut self) -> Reading {
        self.i2c
            .write_async(Self::SHT4X_DEFAULT_ADDR, [self.mode.to_byte()])
            .await
            .unwrap();
        Timer::after(self.mode.get_delay()).await;
        let mut buf = [0u8; 6];
        self.i2c
            .read_async(Self::SHT4X_DEFAULT_ADDR, &mut buf)
            .await
            .unwrap();

        let temp_data = &buf[0..2];
        let humid_data = &buf[3..5];

        let temperature = {
            let temp = (temp_data[1] as u16 + ((temp_data[0] as u16) << 8)) as f32;

            -45.0 + 175.0 * temp / 65535.0
        };

        let humidity = {
            let temp = (humid_data[1] as u16 + ((temp_data[0] as u16) << 8)) as f32; // TODO: this might not be right, unsure as of now

            (-6.0 + 125.0 * temp / 65535.0).clamp(0.0, 100.0)
        };

        Reading::new(temperature, humidity)
    }

    pub async fn serial_number(&mut self) -> u32 {
        self.i2c
            .write_async(Self::SHT4X_DEFAULT_ADDR, [Self::SHT4X_READSERIAL])
            .await
            .unwrap();
        Timer::after_millis(10).await;
        let mut buf = [0u8; 6];
        self.i2c
            .read_async(Self::SHT4X_DEFAULT_ADDR, &mut buf)
            .await
            .unwrap();
        let ser1 = &buf[0..2];
        let ser2 = &buf[3..5];

        ((ser1[0] as u32) << 24)
            + ((ser1[1] as u32) << 16)
            + ((ser2[0] as u32) << 8)
            + ser2[1] as u32
    }
}

// TODO: there is code duplication in both of these tasks, but embassy does not support generics so we are stuck with this as of now, that's alright though!
#[embassy_executor::task]
async fn process_readings_sht(
    mut sensor: SHT40<'static>,
    stack: Stack<'static>,
    options: Options,
    mut ticker: Ticker,
) {
    // Read a few sensor values just to get them out of any buffers they may be present in
    let _ = sensor.get_reading().await;
    let _ = sensor.get_reading().await;
    loop {
        handle_reading_to_webserver(&mut sensor, &options, stack).await;
        ticker.next().await;
    }
}

#[embassy_executor::task]
async fn process_readings_aht(
    mut sensor: AHT20<'static>,
    stack: Stack<'static>,
    options: Options,
    mut ticker: Ticker,
) {
    // Read a few sensor values just to get them out of any buffers they may be present in
    let _ = sensor.get_reading().await;
    let _ = sensor.get_reading().await;
    loop {
        handle_reading_to_webserver(&mut sensor, &options, stack).await;
        ticker.next().await;
    }
}

async fn handle_reading_to_webserver(
    sensor: &mut impl TempHumidSensor,
    options: &Options,
    stack: Stack<'static>,
) {
    let reading = sensor.get_reading().await;

    info!(
        "Reading: temp: {}, humidity: {}",
        reading.temperature, reading.humidity
    );

    let url = {
        // we use 120 as the length to make it ABSOLUTELY have enough capacity for writing the data to it
        let mut url = heapless::String::<120>::from_str(BASE_URL).unwrap();

        // write sensor readings to a heapless string so we can send it as part of the URL
        let mut float_buf = [b'0'; lexical_core::BUFFER_SIZE];
        let temperature_string = lexical_core::write_with_options::<f32, FORMAT>(
            reading.temperature,
            &mut float_buf,
            options,
        );

        // we are going to ignore most of these push operations because we already reduce the size of the floats when writing them to a string, this should only overflow if part of the BASE_URL is too long to begin with
        let _ = url.push_str(core::str::from_utf8(temperature_string).expect("TODO"));

        // append a / between temperature and the humidity as that's what the web server expects
        let _ = url.push('/');

        let mut float_buf = [b'0'; lexical_core::BUFFER_SIZE];
        let humidity_string = lexical_core::write_with_options::<f32, FORMAT>(
            reading.humidity,
            &mut float_buf,
            options,
        );
        let _ = url.push_str(core::str::from_utf8(humidity_string).expect("TODO"));

        url
    };

    info!("Built url: {}", url);

    let mut rx_buffer = [0; 8192];
    let client_state = TcpClientState::<1, 1024, 1024>::new();
    let tcp_client = TcpClient::new(stack, &client_state);
    let dns_client = DnsSocket::new(stack);
    let mut http_client = HttpClient::new(&tcp_client, &dns_client);

    info!("Connecting to url: {}", url);
    let mut request = match http_client.request(Method::GET, &url).await {
        Ok(req) => req,
        Err(e) => {
            error!("Failed to make HTTP request: {:?}", e);
            return; // handle the error
        }
    };

    let response = match request.send(&mut rx_buffer).await {
        Ok(resp) => resp,
        Err(_e) => {
            error!("Failed to send HTTP request");
            return; // handle the error;
        }
    };

    let body = match core::str::from_utf8(response.body().read_to_end().await.unwrap()) {
        Ok(b) => b,
        Err(_e) => {
            error!("Failed to read response body");
            return; // handle the error
        }
    };
    info!("Response body: {:?}", &body);
    info!("Reading: {:?}", reading);
}

pub struct AHT20<'a> {
    i2c: I2c<'a, I2C0, i2c::Async>,
}

#[derive(Debug, Format)]
pub struct Reading {
    pub temperature: f32,
    pub humidity: f32,
}

impl Reading {
    pub fn new(temperature: f32, humidity: f32) -> Self {
        Self {
            temperature,
            humidity,
        }
    }
}

impl<'a> AHT20<'a> {
    const AHT20_I2CADDR: u8 = 0x38;
    #[allow(dead_code)]
    const AHT20_CMD_SOFTRESET: [u8; 1] = [0xBA];
    const AHT20_CMD_INITIALIZE: [u8; 3] = [0xBE, 0x08, 0x00];
    const AHT20_CMD_MEASURE: [u8; 3] = [0xAC, 0x33, 0x00];
    #[allow(dead_code)]
    const AHT20_STATUSBIT_BUSY: u8 = 7;
    const AHT20_STATUSBIT_CALIBRATED: u8 = 3;

    pub async fn new(i2c: I2c<'a, I2C0, i2c::Async>) -> Result<Self, i2c::Error> {
        let mut new_sensor = Self { i2c };
        // init command
        new_sensor
            .i2c
            .write_async(Self::AHT20_I2CADDR, Self::AHT20_CMD_INITIALIZE)
            .await?;

        Timer::after_millis(80).await;

        let mut buf: [u8; 1] = [0];
        // read calibration bit
        new_sensor
            .i2c
            .read_async(Self::AHT20_I2CADDR, &mut buf)
            .await?;

        // the true if calibrated
        let _calibrated = buf[0] >> Self::AHT20_STATUSBIT_CALIBRATED & 1 == 1;

        Timer::after_millis(80).await;

        Ok(new_sensor)
    }

    pub async fn get_reading(&mut self) -> Reading {
        Reading::new(self.get_temperature().await, self.get_humidity().await)
    }

    pub async fn get_temperature(&mut self) -> f32 {
        self.i2c
            .write_async(Self::AHT20_I2CADDR, Self::AHT20_CMD_MEASURE)
            .await
            .unwrap();

        let mut buf: [u8; 7] = [0, 0, 0, 0, 0, 0, 0];

        self.i2c
            .read_async(Self::AHT20_I2CADDR, &mut buf)
            .await
            .unwrap();

        let combined = (((buf[3] & 0xF) as u32) << 16) | ((buf[4] as u32) << 8) | buf[5] as u32;

        combined as f32 / 2u32.pow(20) as f32 * 200.0 - 50.0
    }

    pub async fn get_humidity(&mut self) -> f32 {
        self.i2c
            .write_async(Self::AHT20_I2CADDR, Self::AHT20_CMD_MEASURE)
            .await
            .unwrap();

        let mut buf: [u8; 7] = [0, 0, 0, 0, 0, 0, 0];

        self.i2c
            .read_async(Self::AHT20_I2CADDR, &mut buf)
            .await
            .unwrap();

        let combined = ((buf[1] as u32) << 12) | ((buf[2] as u32) << 4) | ((buf[3] as u32) >> 4);

        combined as f32 * 100.0 / 2u32.pow(20) as f32
    }
}
