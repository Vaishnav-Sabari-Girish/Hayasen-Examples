#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use esp_backtrace as _;
use esp_println::println;
use hayasen::bme280_hayasen;
use esp_hal::{
    i2c::master::{
        I2c, Config
    },
    clock::CpuClock,
    delay::Delay,
    main
};

esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    // generator version: 0.4.0

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    let delay = Delay::new();

    let sda = peripherals.GPIO4;
    let scl = peripherals.GPIO5;

    let bme_address: u8 = 0x76;

    let i2c = I2c::new(peripherals.I2C0, Config::default())
        .unwrap()
        .with_sda(sda)
        .with_scl(scl);

    let mut sensor = bme280_hayasen::create_default(i2c, bme_address).unwrap();

    delay.delay_millis(100);

    loop {
        match bme280_hayasen::read_all(&mut sensor) {
            Ok((temperature, pressure, humidity)) => {
                println!("Temperature: {:.2} C", temperature);
                println!("Pressure : {:.2}", pressure);
                println!("Humidity: {:.2?}", humidity);
            },
            Err(e) => {
                println!("Failed to read sensor data: {:?}", e);
            }
        }

        delay.delay_millis(1000);
    }
}
