#![no_std]
#![no_main]

use cyw43_pio::PioSpi;
use defmt::*;
use embassy_executor::Spawner;
use embassy_rp::{
    bind_interrupts, gpio,
    i2c::{self, I2c, InterruptHandler},
    peripherals::{I2C0, PIO0, UART1},
    uart,
};

use embassy_time::Timer;
use gpio::{Level, Output};
use {defmt_rtt as _, panic_probe as _};
use embassy_rp::pio::Pio;

bind_interrupts!(struct IrqsI2C {
    I2C0_IRQ => InterruptHandler<I2C0>;
});

bind_interrupts!(struct IrqsWifi {
    PIO0_IRQ_0 => embassy_rp::pio::InterruptHandler<PIO0>;
});

static WIFI_NETWORK: &'static str = env!("WIFI_NETWORK_PICO");
static WIFI_PASSWORD: &'static str = env!("WIFI_PASSWORD_PICO");



#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_rp::init(Default::default());


    let fw = include_bytes!("../cyw43-firmware/43439A0.bin");
    let clm = include_bytes!("../cyw43-firmware/43439A0_clm.bin");



    let mut sensor = AHT20::new(I2c::new_async(p.I2C0, p.PIN_5, p.PIN_4, IrqsI2C, i2c::Config::default())).await;

    let pwr = Output::new(p.PIN_23, Level::Low);
    let cs = Output::new(p.PIN_25, Level::High);
    let mut pio = Pio::new(p.PIO0, IrqsWifi);
    let spi = PioSpi::new(&mut pio.common, pio.sm0, pio.irq0, cs, p.PIN_24, p.PIN_29, p.DMA_CH0);



    loop {
        // led.set_high();
        info!(
            "Temp: {}, Humidity: {}",
            sensor.get_temperature().await,
            sensor.get_humidity().await
        );
        // led.set_low();
        Timer::after_millis(1000).await;
    }
}

pub struct AHT20<'a> {
    i2c: I2c<'a, I2C0, i2c::Async>,
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
