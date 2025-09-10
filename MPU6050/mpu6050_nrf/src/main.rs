#![no_std]
#![no_main]

use cortex_m_rt::entry;
use nrf52840_hal::{
    gpio::p0::Parts as P0Parts,
    pac::Peripherals,
    twim,
    Timer,
    Twim
};
use embedded_hal::delay::DelayNs;
use rtt_target::{rprintln, rtt_init_print};
use panic_rtt_target as _;
use hayasen::mpu6050_hayasen;

#[entry]
fn main() -> ! {
    rtt_init_print!();
    rprintln!("RTT Initialized");

    let p = Peripherals::take().unwrap();

    let mpu_address: u8 = 0x68;

    let port0 = P0Parts::new(p.P0);

    let mut timer = Timer::new(p.TIMER0);

    let scl_pin = port0.p0_27.into_floating_input().degrade();
    let sda_pin = port0.p0_26.into_floating_input().degrade();

    let i2c_pins = twim::Pins {
        scl: scl_pin,
        sda: sda_pin
    };

    let i2c = Twim::new(p.TWIM0, i2c_pins, twim::Frequency::K400);

    rprintln!("TWIM0 initialized successfully");

    let mut sensor = hayasen::mpu6050_hayasen::create_default(i2c, mpu_address).unwrap();

    timer.delay_ms(100);
    sensor.disable_sleep().unwrap();
    timer.delay_ms(100);

    loop {
        match mpu6050_hayasen::read_all(&mut sensor) {
            Ok((temperature, acceleration, angular_velocity)) => {
                rprintln!("Temperature : {:.2} C", temperature);
                rprintln!("Acceleration [X, Y, Z] : [{:.3}, {:.3}, {:.3}] g", acceleration[0], acceleration[1], acceleration[2]);
                rprintln!("Angular Velocity [X, Y, Z] : [{:.3}, {:.3}, {:.3}] dps", angular_velocity[0], angular_velocity[1], angular_velocity[2]);
            },
            Err(e) => {
                rprintln!("Failed to read sensor data: {:?}", e);
            }
        }
        timer.delay_ms(500);
    }
}
