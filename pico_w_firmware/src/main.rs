#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]

use core::net::Ipv4Addr;
use core::num;
use core::str::FromStr;
use cyw43::JoinOptions;
use cyw43_pio::{PioSpi, DEFAULT_CLOCK_DIVIDER};
use defmt::*;
use embassy_executor::Spawner;
use embassy_net::dns::DnsSocket;
use embassy_net::tcp::client::{TcpClient, TcpClientState};
use embassy_net::{Ipv4Address, Ipv4Cidr, Stack, StackResources};
use embassy_rp::pio::Pio;
use embassy_rp::{
    bind_interrupts, gpio,
    i2c::{self, I2c, InterruptHandler},
    peripherals::{I2C0, PIO0},
    Peripheral,
};
use embassy_rp::clocks::RoscRng;
use embassy_time::{Duration, Ticker, Timer};
use gpio::{Level, Output};
use heapless::Vec;
use lexical_core::write_float_options::Options;
use rand_core::RngCore;
use reqwless::client::HttpClient;
use reqwless::request::Method;
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};
use sensors::{TempHumidSensor, AHT20, SHT40};
use crate::net_tasks::{cyw43_task, net_task};
use crate::processing_readings::{process_readings_aht, process_readings_sht};

mod net_tasks;
mod sensors;
mod processing_readings;

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

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    info!("Init");

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
        DEFAULT_CLOCK_DIVIDER,
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

    // use a static ip if the environment variable is set, if not use DHCP
    let config = match option_env!("STATIC_IP_ADDRESS") {
        None => embassy_net::Config::dhcpv4(Default::default()),
        Some(ip_env) => {
            let ip = Ipv4Address::from_str(ip_env).unwrap();
            embassy_net::Config::ipv4_static(embassy_net::StaticConfigV4 {
                address: Ipv4Cidr::new(ip, 24),
                dns_servers: Vec::from_slice(&[
                    Ipv4Addr::from_str("1.1.1.1").unwrap(),
                    Ipv4Addr::from_str("8.8.8.8").unwrap(),
                ])
                .unwrap(),
                gateway: Some(Ipv4Address::new(10, 0, 0, 1)),
            })
        }
    };

    let mut rng = RoscRng;

    // network stack seed
    let seed = rng.next_u64();



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
        match control.join(WIFI_NETWORK, JoinOptions::new(WIFI_PASSWORD.as_bytes())).await {
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

