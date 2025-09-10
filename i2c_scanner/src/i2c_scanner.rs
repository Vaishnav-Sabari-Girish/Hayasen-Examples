use embedded_hal::i2c::I2c;
use rtt_target::rprintln;

pub fn scan_i2c_bus<I2C, E>(i2c: &mut I2C)
where
    I2C: I2c<Error = E>,
{
    let mut found = false;
    for addr in 1..=127 {
        let mut buf = [0u8; 1];
        let res = i2c.read(addr, &mut buf);
        if res.is_ok() {
            rprintln!("Device found at address: 0x{:02X}", addr);
            found = true;
        }
    }
    if !found {
        rprintln!("No I2C devices found.");
    }
}
