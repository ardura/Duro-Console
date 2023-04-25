/**************************************************
 * Saturation Algorithms
 **************************************************/

use std::f32::consts::LN_10;

use nih_plug::util;

// tape saturation using transfer function
fn tape_saturation(input_signal: f32, drive: f32, threshold: f32) -> f32 {
    // Define the transfer curve for the tape saturation effect
    let transfer = |x: f32| -> f32 {
        (x * drive).tanh() / (threshold * drive).tanh()
    };

    // Apply the transfer curve to the input sample
    let output_sample = transfer(input_signal);

    // soft clit the output
    let normalized_output_sample = output_sample / (1.0 + output_sample.abs());

    normalized_output_sample
}

// Tanh saturation
fn symmetrical_saturate(sample: f32, drive: f32, threshold: f32) -> f32 {
    let scaled;
    if sample.abs() < threshold {
        scaled = sample;
    } else {
        scaled = sample * drive;
    }
    let saturated = scaled.tanh();
    saturated / saturated.abs().max(1.0)
}

// Chebyshev polynomial saturation
fn chebyshev_tape (sample: f32, drive: f32) -> f32 {
    let k = 1.0 / (1.0 - drive); // saturation limit
    let x = sample * k; // normalized input

    // let x = sample * drive * 10.0;
    let x2 = x * x;
    let x3 = x * x2;
    // let x4 = x2 * x2;
    let x5 = x3 * x2;
    let x6 = x3 * x3;

    let y = x
        - 0.166667 * x3
        + 0.00833333 * x5
        - 0.000198413 * x6;

    y / (1.0 + y.abs()) // Soft clip output
}

// Golden ratio based saturation with cubic curve
fn golden_cubic(sample: f32, threshold: f32) -> f32 {
    let golden_ratio = 1.61803398875;
    let abs_input = sample.abs();
    let output = if abs_input > threshold {
        let sign = sample.signum();
        let excess = abs_input - threshold;
        let shaped_excess = golden_ratio * excess.powi(3); // apply cubic function multiplied by golden ratio
        sign * (threshold + shaped_excess)
    } else {
        sample
    };
    output
}

/**************************************************
 * Duro Compressor
 **************************************************/

pub struct Compressor {
    threshold: f32,
    ratio: i32,
    attack_time_ms: i32,
    release_time_ms: i32,
    attack_coeff: f32,
    release_coeff: f32,
    envelope: f32,
    compressor_type: i32,
    sample_rate: f32,
    prev_gain: f32,
}

impl Compressor {
    pub fn new(
        threshold: f32,
        ratio: i32,
        attack_time_ms: i32,
        release_time_ms: i32,
        compressor_type: i32,
        sample_rate: f32,
    ) -> Self {
        let attack_coeff = (-1.0 / (attack_time_ms as f32 * sample_rate)).exp();
        let release_coeff = (-1.0 / (release_time_ms as f32 * sample_rate)).exp();
        // The 10 is the Hz of the DC offset
        //let dc_alpha = 2.0 * std::f32::consts::PI * 10.0 / sample_rate;
        Self {
            threshold,
            ratio,
            attack_coeff,
            release_coeff,
            attack_time_ms,
            release_time_ms,
            envelope: 0.0,
            compressor_type,
            sample_rate,
            prev_gain: 0.0,
        }
    }

    pub fn update_vals(&mut self, threshold: f32, ratio: i32, attack_time_ms: i32, release_time_ms: i32, compressor_type: i32, sample_rate: f32) {
        self.threshold = threshold;
        self.ratio = ratio;
        self.sample_rate = sample_rate;
        self.attack_coeff = (-1.0 / (attack_time_ms as f32 * sample_rate/1000.0)).exp();
        self.release_coeff = (-1.0 / (release_time_ms as f32 * sample_rate/1000.0)).exp();

        self.compressor_type = compressor_type;
    }

    pub fn duro_compress(&mut self, sample: f32, sat_type: i32, compressor_type: i32) -> f32 {

        //win_dbg_logger::output_debug_string(format!("\nThreshold: {}, Ratio: {}, Attack: {}, Release: {}, Envelope: {}\n\n", self.threshold, self.ratio, self.attack_coeff, self.release_coeff, self.envelope).as_str());

        
        let mut compressed_sample_internal = 0.0;

        // Basic compressor implementation
        if compressor_type == 0
        {
            let coeff_per_sample = 1.0 / (self.sample_rate * 0.001);

            // Compute attack and release coefficients
            let attack_coeff = (1.0 - (-10.0 / (self.attack_time_ms as f32 * self.sample_rate)).exp()).powf(coeff_per_sample);
            let release_coeff = (1.0 - (-10.0 / (self.release_time_ms as f32 * self.sample_rate)).exp()).powf(coeff_per_sample);

            // Compute instantaneous gain based on compression curve
            compressed_sample_internal = sample / self.ratio as f32;
            //x.powf(1.0 / GOLDEN_RATIO);

            // Compute gain coefficient based on attack and release state
            let gain_coeff = if compressed_sample_internal >= self.prev_gain {
                attack_coeff
            } else {
                release_coeff
            };

            // Update state with new gain coefficient and previous output level
            self.prev_gain = gain_coeff * (self.prev_gain - compressed_sample_internal) + compressed_sample_internal;
            self.attack_coeff = attack_coeff;
            self.release_coeff = release_coeff;
        }
        // VCA compressor implementation 1
        else if compressor_type == 1 
        {
            let threshold_db = util::gain_to_db(self.threshold.log10());
            let ratio_db = util::gain_to_db((self.ratio as f32).log10());
            let sample_db = util::gain_to_db(sample.log10());
        
            let gain_db = sample_db - threshold_db - ratio_db;
            let attack_rate = (1.0 / self.attack_time_ms as f32) * std::f32::consts::PI;
            let release_rate = (1.0 / self.release_time_ms as f32) * std::f32::consts::PI;
            
            let mut time = 0.0;
            while time < self.attack_time_ms as f32 {
                self.envelope += attack_rate * time;
                time += 1.0;
            }

            compressed_sample_internal = sample;
            while time < self.release_time_ms as f32 {
                compressed_sample_internal *= self.ratio as f32 * self.envelope;
                self.envelope -= release_rate * time;
                time += 1.0;
            }

            // Lower this signal because it is LOUDER than the original
            compressed_sample_internal *= util::db_to_gain(-20.0);

            compressed_sample_internal *= gain_db;
        }
        // Golden Cubic Compressor
        else if compressor_type == 2
        {
            const GOLDEN_RATIO: f32 = 1.61803398875;
            const KNEE_WIDTH_DB: f32 = 6.0;

            let coeff_per_sample = 1.0 / (self.sample_rate * 0.001);
            let attack_coeff = (1.0 - (-LN_10 / (self.attack_time_ms as f32 * self.sample_rate)).exp()).powf(coeff_per_sample);
            let release_coeff = (1.0 - (-LN_10 / (self.release_time_ms as f32 * self.sample_rate)).exp()).powf(coeff_per_sample);


            let threshold_lin = 10.0_f32.powf(self.threshold / 20.0);
            let level_lin = sample.abs();
    
            let knee_width_lin = 10.0_f32.powf(KNEE_WIDTH_DB / 20.0);
            let mut x = level_lin / threshold_lin;
    
            // Apply soft knee
            if x.abs() >= 1.0 / knee_width_lin {
                x = 1.0 + ((x - 1.0) / self.ratio as f32);
            } else {
                x = (x / knee_width_lin + 1.0 - 1.0 / knee_width_lin).ln() / LN_10 * knee_width_lin + 1.0;
            }
    
            // Apply compression curve
            compressed_sample_internal = threshold_lin * x.powf(1.0 / GOLDEN_RATIO).powi(3);
    
            // Compute gain coefficient based on attack and release state
            let gain_coeff = if compressed_sample_internal >= self.prev_gain {
                self.attack_coeff
            } else {
                self.release_coeff
            };
    
            // Update state with new gain coefficient and previous output level
            self.prev_gain = gain_coeff * (self.prev_gain - compressed_sample_internal) + compressed_sample_internal;
            self.attack_coeff = attack_coeff;
            self.release_coeff = release_coeff;
        }

        match sat_type {
            // No saturation
            0 => return compressed_sample_internal,
            // adding even and odd harmonics
            1 => return tape_saturation(compressed_sample_internal, 0.6, self.threshold),
            // Symmetrical saturation because I learned about it when researching DC Offset
            2 => return symmetrical_saturate(compressed_sample_internal, 0.8, self.threshold),
            // Chebyshev polynomial saturation based off the symmetrical saturation research - pretending to be tape
            3 => return chebyshev_tape(compressed_sample_internal, 0.4),
            // Golden Cubic designed by Ardura
            4 => return golden_cubic(compressed_sample_internal, self.threshold),
            // Default to no saturation
            _ => return compressed_sample_internal,
        }

    }
}
