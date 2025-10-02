#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use defmt::*;
use esp_hal::{
    i2c::master::{
        I2c, Config
    },
    clock::CpuClock,
    delay::Delay,
    main
};
use hayasen::max30102_hayasen::{
    create_default_with_address, 
    read_fifo_batch, 
    read_temperature, 
    start_temperature_measurement,
    setup_high_performance_mode
};
use hayasen::max30102::FifoSample;

use esp_println as _;
use esp_backtrace as _;

esp_bootloader_esp_idf::esp_app_desc!();

struct HeartRateDetector {
    samples: [u32; 8],
    index: usize,
    last_peak_time: u32,
    bpm: u32,
    min_ir: u32,
    max_ir: u32,
    samples_count: u32,
    dc_filter_w: i32,
}

impl HeartRateDetector {
    fn new() -> Self {
        Self {
            samples: [0; 8],
            index: 0,
            last_peak_time: 0,
            bpm: 0,
            min_ir: u32::MAX,
            max_ir: 0,
            samples_count: 0,
            dc_filter_w: 0,
        }
    }

    fn dc_removal(&mut self, x: i32) -> i32 {
        let w = x + (31 * self.dc_filter_w) / 32;
        let result = w - self.dc_filter_w;
        self.dc_filter_w = w;
        result
    }

    fn process_sample(&mut self, ir_value: u32, current_time: u32) -> Option<u32> {
        if ir_value < 1000 {
            return Some(self.bpm);
        }

        self.samples_count += 1;

        let filtered = self.dc_removal(ir_value as i32);
        let filtered_u32 = if filtered > 0 { filtered as u32 } else { 0 };

        if self.samples_count > 100 {
            self.min_ir = self.min_ir.min(filtered_u32);
            self.max_ir = self.max_ir.max(filtered_u32);
            
            if self.samples_count % 500 == 0 {
                self.min_ir = filtered_u32;
                self.max_ir = filtered_u32;
            }
        }
        
        self.samples[self.index] = filtered_u32;
        let old_index = self.index;
        self.index = (self.index + 1) % self.samples.len();
        
        if self.samples_count < self.samples.len() as u32 {
            return Some(self.bpm);
        }

        let signal_range = if self.max_ir > self.min_ir {
            self.max_ir - self.min_ir
        } else {
            1000
        };

        let threshold = self.min_ir + signal_range / 5;

        let current = self.samples[old_index];
        let prev = self.samples[(old_index + self.samples.len() - 1) % self.samples.len()];
        let next = self.samples[(old_index + 1) % self.samples.len()];

        if current > threshold && current > prev && current > next {
            if current > prev + signal_range / 10 && current > next + signal_range / 10 {
                if self.last_peak_time > 0 {
                    let time_diff = current_time.saturating_sub(self.last_peak_time);
                    
                    if time_diff >= 400 && time_diff <= 1500 {
                        let instant_bpm = 60000 / time_diff;
                        
                        if instant_bpm >= 40 && instant_bpm <= 180 {
                            if self.bpm == 0 {
                                self.bpm = instant_bpm;
                            } else {
                                self.bpm = (self.bpm * 2 + instant_bpm * 3) / 5;
                            }
                        }
                    }
                }
                self.last_peak_time = current_time;
            }
        }
        
        Some(self.bpm)
    }
    
    fn get_signal_range(&self) -> u32 {
        if self.max_ir > self.min_ir { 
            self.max_ir - self.min_ir 
        } else { 
            0 
        }
    }

    fn reset_if_no_signal(&mut self) {
        if self.samples_count > 0 && self.samples_count % 1000 == 0 {
            self.bpm = 0;
        }
    }
}

#[main]
fn main() -> ! {
    info!("MAX30102 Heart Rate Monitor");

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    let delay = Delay::new();
    let sda = peripherals.GPIO4;
    let scl = peripherals.GPIO5;

    let i2c = I2c::new(peripherals.I2C0, Config::default())
        .unwrap()
        .with_sda(sda)
        .with_scl(scl);

    let mut sensor = match create_default_with_address(i2c) {
        Ok(mut s) => {
            info!("Sensor initialized successfully!");
            let _ = setup_high_performance_mode(&mut s);
            s
        }
        Err(_) => {
            error!("Failed to initialize sensor");
            loop {
                delay.delay_millis(1000);
            }
        }
    };

    info!("Place finger on sensor and keep still...");

    let mut hr_detector = HeartRateDetector::new();
    let mut sample_buffer: [FifoSample; 16] = core::array::from_fn(|_| FifoSample { red: 0, ir: 0 });

    let mut time_ms: u32 = 0;
    let mut temp_counter = 0;
    let mut current_bpm: u32 = 0;
    let mut last_hr_display = 0;

    loop {
        if let Ok(count) = read_fifo_batch(&mut sensor, &mut sample_buffer) {
            if count > 0 {
                for i in 0..count {
                    if let Some(bpm) = hr_detector.process_sample(sample_buffer[i].ir, time_ms) {
                        current_bpm = bpm;
                    }
                    time_ms += 10;
                }
            }
        }

        if time_ms.saturating_sub(last_hr_display) >= 2000 {
            last_hr_display = time_ms;
            
            if current_bpm > 0 {
                info!("💓 Heart Rate: {} BPM", current_bpm);
            } else {
                let signal_range = hr_detector.get_signal_range();
                if signal_range < 500 {
                    info!("⚠️  Place finger firmly on sensor");
                } else {
                    info!("🔍 Detecting heartbeat...");
                }
            }
            
            hr_detector.reset_if_no_signal();
        }

        temp_counter += 1;
        if temp_counter >= 250 {
            temp_counter = 0;
            let _ = start_temperature_measurement(&mut sensor);
            delay.delay_millis(30);

            if let Ok(Some(temp)) = read_temperature(&mut sensor) {
                info!("🌡️  Temperature: {}°C", temp);
            }
        }

        delay.delay_millis(20);
    }
}
