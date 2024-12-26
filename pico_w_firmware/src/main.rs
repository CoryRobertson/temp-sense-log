#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]

use core::net::Ipv4Addr;
use core::str::FromStr;
use cyw43_pio::PioSpi;
use defmt::*;
use embassy_executor::Spawner;
use embassy_rp::{
    bind_interrupts, gpio,
    i2c::{self, I2c, InterruptHandler},
    peripherals::{I2C0, PIO0, UART1},
    uart,
};
use embassy_rp::peripherals::DMA_CH0;
use embassy_time::{Duration, Ticker, Timer};
use gpio::{Level, Output};
use {defmt_rtt as _, panic_probe as _};
use embassy_rp::pio::Pio;
use static_cell::StaticCell;
use embassy_net::{Ipv4Address, Ipv4Cidr, Runner, Stack, StackResources};
use embassy_net::dns::DnsSocket;
use embassy_net::tcp::client::{TcpClient, TcpClientState};
use embassy_rp::clocks::RoscRng;
use heapless::Vec;
use reqwless::client::{HttpClient, TlsConfig, TlsVerify};
use reqwless::request::Method;
use rand_core::RngCore;

bind_interrupts!(struct IrqsI2C {
    I2C0_IRQ => InterruptHandler<I2C0>;
});

bind_interrupts!(struct IrqsWifi {
    PIO0_IRQ_0 => embassy_rp::pio::InterruptHandler<PIO0>;
});

pub static WIFI_NETWORK: &'static str = env!("WIFI_NETWORK_PICO");
pub static WIFI_PASSWORD: &'static str = env!("WIFI_PASSWORD_PICO");

#[embassy_executor::task]
async fn cyw43_task(runner: cyw43::Runner<'static, Output<'static>, PioSpi<'static, PIO0, 0, DMA_CH0>>) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, cyw43::NetDriver<'static>>) -> ! {
    runner.run().await
}


#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    let mut rng = RoscRng;


    // To make flashing faster for development, you may want to flash the firmwares independently
    // at hardcoded addresses, instead of baking them into the program with `include_bytes!`:
    //     probe-rs download ../../cyw43-firmware/43439A0.bin --binary-format bin --chip RP2040 --base-address 0x10100000
    //     probe-rs download cyw43-firmware/43439A0.bin --binary-format bin --chip RP2040 --base-address 0x10100000
    //     probe-rs download ../../cyw43-firmware/43439A0_clm.bin --binary-format bin --chip RP2040 --base-address 0x10140000
    //     probe-rs download cyw43-firmware/43439A0_clm.bin --binary-format bin --chip RP2040 --base-address 0x10140000
    let fw = unsafe { core::slice::from_raw_parts(0x10100000 as *const u8, 230321) };
    let clm = unsafe { core::slice::from_raw_parts(0x10140000 as *const u8, 4752) };


    // let fw = include_bytes!("../cyw43-firmware/43439A0.bin");
    // let clm = include_bytes!("../cyw43-firmware/43439A0_clm.bin");




    // networking pins
    let pwr = Output::new(p.PIN_23, Level::Low);
    let cs = Output::new(p.PIN_25, Level::High);
    let mut pio = Pio::new(p.PIO0, IrqsWifi);
    let spi = PioSpi::new(&mut pio.common, pio.sm0, pio.irq0, cs, p.PIN_24, p.PIN_29, p.DMA_CH0);

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
       address: Ipv4Cidr::new(Ipv4Address::new(10, 0, 0, 224), 24),
       dns_servers: Vec::from_slice(&[Ipv4Addr::from_str("1.1.1.1").unwrap(),Ipv4Addr::from_str("8.8.8.8").unwrap()]).unwrap(),
       gateway: Some(Ipv4Address::new(10, 0, 0, 1)),
    });

    // network stack seed
    let seed = RoscRng::next_u32(&mut RoscRng);

    // network stack
    static RESOURCES: StaticCell<StackResources<5>> = StaticCell::new();
    let (stack, runner) = embassy_net::new(net_device, config, RESOURCES.init(StackResources::new()), seed as u64);

    unwrap!(spawner.spawn(net_task(runner)));


    loop {
        match control
            .join_wpa2(WIFI_NETWORK, WIFI_PASSWORD)
            .await
        {
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

    let mut sensor = AHT20::new(I2c::new_async(p.I2C0, p.PIN_5, p.PIN_4, IrqsI2C, i2c::Config::default())).await;

    let mut ticker = Ticker::every(Duration::from_secs(60));

    loop {

        let url = "http://10.0.0.132:8080/reading/abcd/4.2/5.2";
        // let url = "http://google.com";
        let mut rx_buffer = [0; 8192];
        let mut tls_read_buffer = [0; 16640];
        let mut tls_write_buffer = [0; 16640];
        let client_state = TcpClientState::<1, 1024, 1024>::new();
        let tcp_client = TcpClient::new(stack, &client_state);
        let dns_client = DnsSocket::new(stack);
        let tls_config = TlsConfig::new(seed as u64, &mut tls_read_buffer, &mut tls_write_buffer, TlsVerify::None);
        let mut http_client = HttpClient::new(&tcp_client, &dns_client);

        let reading = sensor.get_reading().await;

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
        ticker.next().await;
    }

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
            temperature, humidity
        }
    }
}

impl<'a> AHT20<'a> {
    const AHT20_I2CADDR: u8 = 0x38;
    const AHT20_CMD_SOFTRESET: [u8; 1] = [0xBA];
    const AHT20_CMD_INITIALIZE: [u8; 3] = [0xBE, 0x08, 0x00];
    const AHT20_CMD_MEASURE: [u8; 3] = [0xAC, 0x33, 0x00];
    const AHT20_STATUSBIT_BUSY: u8 = 7;
    const AHT20_STATUSBIT_CALIBRATED: u8 = 3;

    pub async fn new(i2c: I2c<'a, I2C0, i2c::Async>) -> Self {
        let mut new_sensor = Self { i2c };
        // init command
        new_sensor
            .i2c
            .write_async(Self::AHT20_I2CADDR, Self::AHT20_CMD_INITIALIZE)
            .await
            .unwrap();

        Timer::after_millis(80).await;

        let mut buf: [u8; 1] = [0];
        // read calibration bit
        new_sensor
            .i2c
            .read_async(Self::AHT20_I2CADDR, &mut buf)
            .await
            .unwrap();

        // the true if calibrated
        let _calibrated = buf[0] >> Self::AHT20_STATUSBIT_CALIBRATED & 1 == 1;

        Timer::after_millis(80).await;

        new_sensor
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
