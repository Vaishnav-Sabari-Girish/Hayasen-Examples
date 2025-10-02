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

struct SpO2Detector {
    red_ac_sum: i64,
    ir_ac_sum: i64,
    red_dc_sum: i64,
    ir_dc_sum: i64,
    sample_count: u32,
    red_dc_filter: i32,
    ir_dc_filter: i32,
    spo2_value: u32,
}

impl SpO2Detector {
    fn new() -> Self {
        Self {
            red_ac_sum: 0,
            ir_ac_sum: 0,
            red_dc_sum: 0,
            ir_dc_sum: 0,
            sample_count: 0,
            red_dc_filter: 0,
            ir_dc_filter: 0,
            spo2_value: 0,
        }
    }

    // Separate DC filter methods to avoid borrow checker issues
    fn red_dc_filter(&mut self, x: i32) -> i32 {
        let w = x + (15 * self.red_dc_filter) / 16;
        let result = w - self.red_dc_filter;
        self.red_dc_filter = w;
        result
    }

    fn ir_dc_filter(&mut self, x: i32) -> i32 {
        let w = x + (15 * self.ir_dc_filter) / 16;
        let result = w - self.ir_dc_filter;
        self.ir_dc_filter = w;
        result
    }

    fn process_sample(&mut self, red: u32, ir: u32) -> Option<u32> {
        if red < 1000 || ir < 1000 {
            return Some(self.spo2_value);
        }

        // Apply DC filters using separate methods
        let red_dc = self.red_dc_filter(red as i32);
        let ir_dc = self.ir_dc_filter(ir as i32);

        // Accumulate AC (filtered) and DC (original) values
        self.red_ac_sum += red_dc.abs() as i64;
        self.ir_ac_sum += ir_dc.abs() as i64;
        self.red_dc_sum += red as i64;
        self.ir_dc_sum += ir as i64;
        self.sample_count += 1;

        // Calculate SpO2 every 100 samples
        if self.sample_count >= 100 {
            let red_ac_avg = self.red_ac_sum / self.sample_count as i64;
            let ir_ac_avg = self.ir_ac_sum / self.sample_count as i64;
            let red_dc_avg = self.red_dc_sum / self.sample_count as i64;
            let ir_dc_avg = self.ir_dc_sum / self.sample_count as i64;

            // Calculate R ratio (Red AC/DC divided by IR AC/DC)
            if red_dc_avg > 0 && ir_dc_avg > 0 && ir_ac_avg > 0 {
                let red_ratio = (red_ac_avg * 1000) / red_dc_avg;
                let ir_ratio = (ir_ac_avg * 1000) / ir_dc_avg;

                if ir_ratio > 0 {
                    let r_ratio = (red_ratio * 1000) / ir_ratio;

                    // SpO2 calibration formula (empirically derived)
                    // SpO2 = 104 - 17 * R
                    let spo2_calc = 104000 - (17 * r_ratio);
                    let spo2_percent = spo2_calc / 1000;

                    // Clamp SpO2 to reasonable range (70-100%)
                    let spo2_final = if spo2_percent < 70 {
                        70
                    } else if spo2_percent > 100 {
                        100
                    } else {
                        spo2_percent as u32
                    };

                    // Simple averaging filter
                    if self.spo2_value == 0 {
                        self.spo2_value = spo2_final;
                    } else {
                        self.spo2_value = (self.spo2_value * 3 + spo2_final) / 4;
                    }
                }
            }

            // Reset accumulators
            self.red_ac_sum = 0;
            self.ir_ac_sum = 0;
            self.red_dc_sum = 0;
            self.ir_dc_sum = 0;
            self.sample_count = 0;
        }

        Some(self.spo2_value)
    }

    fn get_signal_quality(&self, red: u32, ir: u32) -> bool {
        // Consider signal good if both values are above threshold
        red > 5000 && ir > 5000
    }
}

#[main]
fn main() -> ! {
    info!("MAX30102 Health Monitor");
    info!("Heart Rate | Temperature | SpO2");

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
    let mut spo2_detector = SpO2Detector::new();
    let mut sample_buffer: [FifoSample; 16] = core::array::from_fn(|_| FifoSample { red: 0, ir: 0 });

    let mut time_ms: u32 = 0;
    let mut temp_counter = 0;
    let mut current_bpm: u32 = 0;
    let mut current_spo2: u32 = 0;
    let mut current_temp: f32 = 0.0;
    let mut last_display = 0;
    let mut display_phase = 0; // 0 = HR, 1 = Temp, 2 = SpO2

    loop {
        if let Ok(count) = read_fifo_batch(&mut sensor, &mut sample_buffer) {
            if count > 0 {
                for i in 0..count {
                    let red = sample_buffer[i].red;
                    let ir = sample_buffer[i].ir;

                    // Process for heart rate
                    if let Some(bpm) = hr_detector.process_sample(ir, time_ms) {
                        current_bpm = bpm;
                    }

                    // Process for SpO2
                    if let Some(spo2) = spo2_detector.process_sample(red, ir) {
                        current_spo2 = spo2;
                    }
                    
                    time_ms += 10;
                }
            }
        }

        // Display readings sequentially every 3 seconds
        if time_ms.saturating_sub(last_display) >= 3000 {
            last_display = time_ms;
            
            match display_phase {
                0 => {
                    // Display Heart Rate
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
                    display_phase = 1;
                }
                1 => {
                    // Display Temperature
                    if current_temp > 0.0 {
                        info!("🌡️  Temperature: {}°C", current_temp);
                    } else {
                        info!("🌡️  Reading temperature...");
                    }
                    display_phase = 2;
                }
                2 => {
                    // Display SpO2
                    if current_spo2 > 0 {
                        let red_sample = if sample_buffer.len() > 0 { sample_buffer[0].red } else { 0 };
                        let ir_sample = if sample_buffer.len() > 0 { sample_buffer[0].ir } else { 0 };
                        
                        if spo2_detector.get_signal_quality(red_sample, ir_sample) {
                            info!("🫁 SpO2: {}%", current_spo2);
                        } else {
                            info!("🫁 Improving SpO2 signal...");
                        }
                    } else {
                        info!("🫁 Calculating SpO2...");
                    }
                    display_phase = 0;
                }
                _ => {
                    display_phase = 0;
                }
            }
            
            hr_detector.reset_if_no_signal();
        }

        // Read temperature every 250 cycles (about every 5 seconds)
        temp_counter += 1;
        if temp_counter >= 250 {
            temp_counter = 0;
            let _ = start_temperature_measurement(&mut sensor);
            delay.delay_millis(30);

            if let Ok(Some(temp)) = read_temperature(&mut sensor) {
                current_temp = temp;
            }
        }

        delay.delay_millis(20);
    }
}
