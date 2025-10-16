#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use esp_backtrace as _;
use log::{info, error};
use esp_hal::{
    clock::CpuClock,
    delay::Delay,
    gpio::{DriveMode, Output, OutputConfig, Pull},
    main
};

use hayasen::{dhtx_hayasen, Error};

esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    // generator version: 0.6.0

    esp_println::logger::init_logger_from_env();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    let od_config = OutputConfig::default()
        .with_drive_mode(DriveMode::OpenDrain)
        .with_pull(Pull::None);

    let od_for_dht22 = Output::new(
        peripherals.GPIO4,
        esp_hal::gpio::Level::High,
        od_config
    )
        .into_flex();

    od_for_dht22.peripheral_input();

    let delay = Delay::new();

    let mut dht22 = match dhtx_hayasen::create_dht22(od_for_dht22, delay) {
        Ok(sensor) => {
            info!("DHT22 initialized successfully");
            sensor
        },
        Err(e) => {
            error!("Failed to initialized DHT22: {:?}", e);
            loop {
                //
            }
        }
    };


    loop {
        delay.delay_millis(2000);

        info!("");

        match dhtx_hayasen::read_all(&mut dht22) {
            Ok((temperature, humidity)) => info!(
                "DHT22 sensor - Temperature : {:.1} C , Humidity : {:.1} %",
                temperature,
                humidity
            ),
            Err(Error::InvalidData) => error!("An error occurred: Invalid data/checksum error"),
            Err(Error::SensorSpecific(msg)) => error!("An error occurred: DHT22 {}", msg),
            Err(Error::NotDetected) => error!("An error occurred: Sensor not detected"),
            Err(_) => error!("An error occurred while trying to read the sensor"),
        }
    }

}
