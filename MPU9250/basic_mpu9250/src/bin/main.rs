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

    let mut i2c = I2c::new(peripherals.I2C0, Config::default())
        .unwrap()
        .with_sda(sda)
        .with_scl(scl);

    // I2C Scanner
    println!("Scanning I2C bus...");
    for addr in 0x08..0x78 {
        if i2c.write(addr, &[]).is_ok() {
            println!("Found device at address: 0x{:02X}", addr);
        }
    }

    // Manual WHO_AM_I test
    println!("Testing WHO_AM_I register manually...");
    let mut who_am_i = [0u8; 1];
    match i2c.write_read(mpu_address, &[0x75], &mut who_am_i) {
        Ok(_) => {
            println!("WHO_AM_I register read successful: 0x{:02X}", who_am_i[0]);
            println!("Expected: 0x71, Got: 0x{:02X}", who_am_i[0]);
            
            if who_am_i[0] == 0x71 {
                println!("WHO_AM_I matches expected value!");
            } else {
                println!("WHO_AM_I does NOT match expected value!");
            }
        },
        Err(e) => {
            println!("Failed to read WHO_AM_I: {:?}", e);
            loop {
                delay.delay_millis(1000);
            }
        }
    }

    // Add some delay before trying to create sensor
    println!("Waiting before sensor initialization...");
    delay.delay_millis(100);

    // Now try to create the sensor
    println!("Attempting to create sensor...");
    let mut sensor = match mpu9250_hayasen::create_default(i2c, mpu_address) {
        Ok(s) => {
            println!("Sensor created successfully!");
            s
        },
        Err(e) => {
            println!("Failed to create sensor: {:?}", e);
            loop {
                delay.delay_millis(1000);
            }
        }
    };

    let (temperature, acceleration, angular_velocity) = mpu9250_hayasen::read_all(&mut sensor).unwrap();

    loop {
        println!("Temperature : {:.2} C", temperature);
        println!("Acceleration [X, Y, Z] : [{:.3}, {:.3}, {:.3}] g", acceleration[0], acceleration[1], acceleration[2]);
        println!("Angular Velocity [X, Y, Z] : [{:.3}, {:.3}, {:.3}] dps", angular_velocity[0], angular_velocity[1], angular_velocity[2]);

        delay.delay_millis(1000);
    }
}
