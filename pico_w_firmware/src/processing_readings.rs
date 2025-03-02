use defmt::warn;
use embassy_net::Stack;
use lexical_core::write_float_options::Options;
use embassy_time::{Instant, Ticker, Timer};
use embassy_futures::select::{select, Either};
use crate::handle_reading_to_webserver;
use crate::sensors::{TempHumidSensor, AHT20, SHT40};

// TODO: there is code duplication in both of these tasks, but embassy does not support generics so we are stuck with this as of now, that's alright though!
#[embassy_executor::task]
pub async fn process_readings_sht(
    sensor: SHT40<'static>,
    stack: Stack<'static>,
    options: Options,
    ticker: Ticker,
) {
    process_readings(sensor,stack,options,ticker).await;
}

pub async fn process_readings(
    mut sensor: impl TempHumidSensor,
    stack: Stack<'static>,
    options: Options,
    mut ticker: Ticker,
) {
    // Read a few sensor values just to get them out of any buffers they may be present in
    let _ = sensor.get_reading().await;
    let _ = sensor.get_reading().await;
    let start_time = Instant::now();
    loop {
        // 60 seconds * 60 -> 1 hour * 4 -> 4 hours
        // testing if setting a limit on uptime for a quick restart will allow for longer reliability
        if Instant::now().duration_since(start_time).as_secs() > 60*60*4 {
            cortex_m::peripheral::SCB::sys_reset();
        }

        match select(
            handle_reading_to_webserver(&mut sensor, &options, stack),
            Timer::after_secs(10),
        )
            .await
        {
            Either::First(_) => {}
            Either::Second(_) => {
                warn!("Triggering an MCU system reset because reading to web server took too long");
                Timer::after_secs(1).await;
                cortex_m::peripheral::SCB::sys_reset();
            }
        }

        ticker.next().await;
    }
}



#[embassy_executor::task]
pub async fn process_readings_aht(
    sensor: AHT20<'static>,
    stack: Stack<'static>,
    options: Options,
    ticker: Ticker,
) {
    process_readings(sensor,stack,options,ticker).await;
}