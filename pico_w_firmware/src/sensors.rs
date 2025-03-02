use embassy_rp::i2c::I2c;
use embassy_rp::peripherals::I2C0;
use embassy_rp::i2c;
use embassy_time::{Duration, Timer};
use core::future::Future;
use defmt::Format;
pub struct SHT40<'a> {
    i2c: I2c<'a, I2C0, i2c::Async>,
    mode: SHT40Mode,
}

impl TempHumidSensor for SHT40<'_> {
    async fn get_reading(&mut self) -> Reading {
        self.measurement().await
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

impl TempHumidSensor for AHT20<'_> {
    async fn get_reading(&mut self) -> Reading {
        self.get_reading().await
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