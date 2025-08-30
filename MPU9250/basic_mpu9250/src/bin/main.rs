#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use esp_backtrace as _;
use esp_hal::{
    i2c::master::{
        I2c, Config
    },
    clock::CpuClock,
    delay::Delay,
    main
};
use esp_println::println;
use hayasen::prelude::*;
use hayasen::mpu9250_hayasen;

esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    let delay = Delay::new();

    let sda = peripherals.GPIO4;
    let scl = peripherals.GPIO5;

    let mpu_address: u8 = 0x68;

    let i2c = I2c::new(peripherals.I2C0, Config::default())
        .unwrap()
        .with_sda(sda)
        .with_scl(scl);

    // Create sensor manually and test each step
    let mut sensor = mpu9250_hayasen::create_default(i2c, mpu_address).unwrap();
    
    // Now try reading data
    loop {
        match mpu9250_hayasen::read_all(&mut sensor) {
            Ok((temperature, acceleration, angular_velocity)) => {
                println!("Temperature : {:.2} C", temperature);
                println!("Acceleration [X, Y, Z] : [{:.3}, {:.3}, {:.3}] g", acceleration[0], acceleration[1], acceleration[2]);
                println!("Angular Velocity [X, Y, Z] : [{:.3}, {:.3}, {:.3}] dps", angular_velocity[0], angular_velocity[1], angular_velocity[2]);
            },
            Err(e) => {
                println!("Failed to read sensor data: {:?}", e);
            }
        }
        delay.delay_millis(1000);
    }
}
