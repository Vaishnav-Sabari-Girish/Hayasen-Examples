#![no_std]
#![no_main]

use cortex_m_rt::entry;
use panic_rtt_target as _;
use rtt_target::rtt_init_print;

use nrf52840_hal::{gpio, pac::Peripherals, twim::{self, Twim}};
use embedded_hal::i2c::I2c;

mod i2c_scanner; // Your I2C scanner module from before

#[entry]
fn main() -> ! {
    rtt_init_print!();

    let p = Peripherals::take().unwrap();
    let port0 = gpio::p0::Parts::new(p.P0);
    
    // I2C pins from first code snippet
    let sda_pin = port0.p0_26.into_floating_input().degrade();
    let scl_pin = port0.p0_27.into_floating_input().degrade();
    let i2c_pins = twim::Pins { scl: scl_pin, sda: sda_pin };
    
    // Create Twim instance as I2C bus
    let mut i2c = Twim::new(p.TWIM0, i2c_pins, twim::Frequency::K100);

    // Pass the I2C peripheral to the scanner
    i2c_scanner::scan_i2c_bus(&mut i2c);

    loop {}
}
