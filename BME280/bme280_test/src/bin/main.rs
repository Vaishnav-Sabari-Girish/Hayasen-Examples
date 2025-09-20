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
    i2c::master::{I2c, Config},
    clock::CpuClock,
    delay::Delay,
    main
};
use embedded_hal::delay::DelayNs;
esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);
    let delay = Delay::new();
    
    let sda = peripherals.GPIO4;
    let scl = peripherals.GPIO5;
    let bme_address: u8 = 0x76; // Change to 0x77 if needed
    
    println!("Starting BME280 sensor...");
    
    let i2c = I2c::new(peripherals.I2C0, Config::default())
        .unwrap()
        .with_sda(sda)
        .with_scl(scl);
    
    // Initial delay to let sensor power up
    delay.delay_millis(100);
    
    // Create and initialize the sensor
    let mut sensor = match bme280_hayasen::create_default(i2c, bme_address) {
        Ok(s) => {
            println!("Sensor created successfully at address 0x{:02X}", bme_address);
            s
        },
        Err(e) => {
            println!("Failed to create sensor at 0x{:02X}: {:?}", bme_address, e);
            println!("Try changing bme_address to 0x77 if using 0x76, or vice versa");
            panic!("Sensor creation failed. Check wiring and I2C address.");
        }
    };
    
    // CRITICAL: Add delay after initialization to let sensor stabilize
    delay.delay_millis(2000);
    
    println!("BME280 sensor initialized successfully!");
    println!("Device type: {:?}", sensor.get_device_type());
    
    if sensor.has_humidity() {
        println!("Humidity sensing available (BME280)");
    } else {
        println!("No humidity sensing (BMP280)");
    }
    
    println!("Starting measurements...");
    
    loop {
        match bme280_hayasen::read_all(&mut sensor) {
            Ok((temperature, pressure, humidity)) => {
                println!("Temperature: {:.2} °C", temperature);
                println!("Pressure: {:.2} hPa", pressure);
                match humidity {
                    Some(h) => println!("Humidity: {:.2} %RH", h),
                    None => println!("Humidity: Not available"),
                }
                println!("---");
            },
            Err(e) => {
                println!("Failed to read sensor data: {:?}", e);
            }
        }
        delay.delay_millis(2000);
    }
}
