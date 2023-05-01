use nih_plug::{util::{self}, prelude::Enum};
use std::{f32::consts::{LN_2, PI}, cmp::max};


#[derive(Enum, PartialEq, Eq, Debug, Copy, Clone)]
pub enum ThresholdMode {
    #[name = "Linear"]
    LINEAR,
    #[name = "VCA"]
    VCA,
    #[name = "Golden Cube"]
    GOLDENCUBE,
}

#[derive(Enum, PartialEq, Eq, Debug, Copy, Clone)]
pub enum SaturationModeEnum {
    #[name = "None"]
    NONESAT,
    #[name = "Tape"]
    TAPESAT,
    #[name = "Symmetrical"]
    SYMMETRICAL,
    #[name = "Chebyshev"]
    CHEBYSHEV,
    #[name = "Golden Cubic"]
    GOLDENCUBIC,
}

/**************************************************
 * Saturation Algorithms
 **************************************************/

// tape saturation using transfer function
fn tape_saturation(input_signal: f32, drive: f32, threshold: f32) -> f32 {
    // Define the transfer curve for the tape saturation effect
    let transfer = |x: f32| -> f32 {
        (x * drive).tanh() / (threshold * drive).tanh()
    };
    // Apply the transfer curve to the input sample
    let output_sample = transfer(input_signal);
    // soft clip the output
    let mut normalized_output_sample = output_sample / (1.0 + output_sample.abs());
    // Lower this signal because it is LOUDER than the original
    normalized_output_sample *= util::db_to_gain(-12.0);
    normalized_output_sample
}

// Tanh saturation
fn symmetrical_saturate(sample: f32, drive: f32, threshold: f32) -> f32 {
    let scaled;
    // Only apply drive when we are above the threshold - this gives different loudness character
    if sample.abs() < threshold {
        scaled = sample;
    } else {
        scaled = sample * drive;
    }
    // Take the hyperbolic tangent of our scaled sample, then normalize it
    let saturated = scaled.tanh();
    saturated / saturated.abs().max(1.0)
}

// Chebyshev polynomial saturation (Thanks to AI help)
fn chebyshev_tape (sample: f32, drive: f32) -> f32 {
    // saturation limit value
    //let k = 1.0 / (1.0 - drive);
    let k = 1.0 / (1.0 + drive);
    // normalized input
    let x = sample * k;
    // Calculate the Chebyshev values
    let x2 = x * x;
    let x3 = x * x2;
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
    // If we are above the threshold, multiply by the golden and cube the excess sample
    let output = if abs_input > threshold {
        let sign = sample.signum();
        let excess = abs_input - threshold;
        let shaped_excess = threshold * golden_ratio * excess.powi(3); // apply cubic function multiplied by golden ratio
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
    envelope: f32,
    compressor_type: crate::duro_process::ThresholdMode,
    sample_rate: f32,
    prev_gain: f32,
}

#[allow(unused_variables)]
impl Compressor {
    pub fn new(
        threshold: f32,
        ratio: i32,
        attack_time_ms: i32,
        release_time_ms: i32,
        compressor_type: crate::duro_process::ThresholdMode,
        sample_rate: f32,
    ) -> Self {
        Self {
            threshold,
            ratio,
            attack_time_ms,
            release_time_ms,
            envelope: 0.0001,
            compressor_type: crate::duro_process::ThresholdMode::LINEAR,
            sample_rate,
            prev_gain: 0.0,
        }
    }

    pub fn update_vals(&mut self, threshold: f32, ratio: i32, attack_time_ms: i32, release_time_ms: i32, compressor_type: crate::duro_process::ThresholdMode, sample_rate: f32) {
        self.threshold = threshold;
        self.ratio = ratio;
        self.sample_rate = sample_rate;
        self.compressor_type = compressor_type;
        self.attack_time_ms = attack_time_ms;
        self.release_time_ms = release_time_ms;
    }

    pub fn duro_compress(&mut self, sample: f32, sat_type: crate::duro_process::SaturationModeEnum, compressor_type: crate::duro_process::ThresholdMode) -> f32 
    {
        // Initialize our return value
        let mut compressed_sample_internal = 0.0;

        // Basic compressor implementation - linear ratio
        if compressor_type == crate::duro_process::ThresholdMode::LINEAR
        {
            let coeff_per_sample = 1.0 / (self.sample_rate * 0.001);

            // Compute attack and release coefficients
            let attack_coeff = (1.0 - (-10.0 / (self.attack_time_ms as f32 * self.sample_rate)).exp()).powf(coeff_per_sample);
            let release_coeff = (1.0 - (-10.0 / (self.release_time_ms as f32 * self.sample_rate)).exp()).powf(coeff_per_sample);

            // Compute instantaneous gain based on compression curve
            compressed_sample_internal = sample / self.ratio as f32;

            // Compute gain coefficient based on attack and release state
            let gain_coeff = if compressed_sample_internal >= self.prev_gain {
                attack_coeff
            } else {
                release_coeff
            };

            //compressed_sample_internal = gain_coeff * compressed_sample_internal;
            compressed_sample_internal = gain_coeff * (self.prev_gain - compressed_sample_internal) + compressed_sample_internal;

            // Balance this signal volume near original
            //compressed_sample_internal *= util::db_to_gain(12.0);

            // Update state with new gain coefficient and previous output level
            self.prev_gain = gain_coeff * (self.prev_gain - compressed_sample_internal) + compressed_sample_internal;
        }
        // Feed forward analog-like compressor
        else if compressor_type == crate::duro_process::ThresholdMode::VCA
        {
            let sample_db = util::gain_to_db(sample.abs());
            let gain_db = sample_db - self.threshold;
            let coeff_per_sample = 1.0 / self.sample_rate / 0.001;
    
            // Compute attack and release coefficients
            let attack_rate = (1.0 - (-10.0 / (self.attack_time_ms as f32 * 0.001 * self.sample_rate)).exp()).powf(coeff_per_sample);
            let release_rate = (1.0 - (-10.0 / (self.release_time_ms as f32 * 0.001 * self.sample_rate)).exp()).powf(coeff_per_sample);
    
            // Update envelope
            if sample_db > self.envelope {
                self.envelope = self.envelope * attack_rate + (1.0 - attack_rate) * sample_db;
            } else {
                self.envelope = f32::max(self.envelope * release_rate + (1.0 - release_rate) * sample_db, sample_db);
            }
    
            // Compute gain reduction
            let envelope_db = util::gain_to_db(self.envelope);
            let gain_reduction_db = if envelope_db > self.threshold {
                self.threshold + (envelope_db - self.threshold) / self.ratio as f32 - sample_db.max(0.0)
            } else {
                0.0
            };
    
            // Apply gain reduction
            let gain_reduction = util::db_to_gain(gain_reduction_db);
            compressed_sample_internal = sample.abs() * gain_reduction * sample.signum();
            //compressed_sample_internal = sample.signum() * sample.abs().max(0.0) * gain_reduction;

        }
        // Golden Cubic Compressor - Harsher Ardura Sound
        else if compressor_type == crate::duro_process::ThresholdMode::GOLDENCUBE
        {
            // Golden ratio since it's cool
            const GOLDEN_RATIO: f32 = 1.61803398875;
            // This causes distortion the smaller it is
            let knee_width_db = 12.0;
            

            let ratio;
            // Calculate with our chosen ratio multiplied in, no real reasoning
            ratio = GOLDEN_RATIO * self.ratio as f32;

            let knee_width = util::db_to_gain(knee_width_db);

            // Since we are measuring amplitude we can use abs - I was stuck trying to fix negative values for a bit on this
            let input_db = util::gain_to_db(sample.abs());

            // Calculate envelope values
            //let gain_attack = f32::exp(-1.0 / (self.sample_rate * self.attack_time_ms as f32/1000.0));
            //let gain_release = f32::exp(-1.0 / (self.sample_rate * self.release_time_ms as f32/1000.0));

            let coeff_per_sample = 1.0 / (self.sample_rate * 0.001);

            // Compute attack and release coefficients
            let gain_attack = (1.0 - (-10.0 / (self.attack_time_ms as f32 * self.sample_rate)).exp()).powf(coeff_per_sample);
            let gain_release = (1.0 - (-10.0 / (self.release_time_ms as f32 * self.sample_rate)).exp()).powf(coeff_per_sample);

            let mut env_out = 0.0;
            let env_in = input_db;

            if env_out < env_in {
                env_out = env_in + gain_attack * (env_out - env_in);
            } else {
                env_out = env_in + gain_release * (env_out - env_in);
            }

            // Calculate compression amount based off input volume vs threshold
            let mut gain_db = 0.0;
            if input_db > self.threshold {
                let x = (input_db - self.threshold) / knee_width_db;
                let x_cubed = x * x * x;
                let mut slope = (ratio - 1.0) * x_cubed + 1.0;
                slope *= LN_2 * knee_width / (PI * 4.0);
                gain_db = self.prev_gain + slope;
                if gain_db > 0.0 {
                    gain_db = 0.0;
                }
            }

            // Store the gain for next sample reference
            self.prev_gain = gain_db;

            // Apply the gain reduction
            let gain = util::db_to_gain(gain_db);
            compressed_sample_internal = sample * gain * env_out;

            // Balance this signal volume near original
            compressed_sample_internal *= util::db_to_gain(-6.0);
        }

        #[allow(unreachable_patterns)]
        match sat_type {
            // No saturation
            SaturationModeEnum::NONESAT => return compressed_sample_internal,
            // adding even and odd harmonics
            SaturationModeEnum::TAPESAT => return tape_saturation(compressed_sample_internal, 0.6, self.threshold),
            // Symmetrical saturation because I learned about it when researching DC Offset
            SaturationModeEnum::SYMMETRICAL => return symmetrical_saturate(compressed_sample_internal, 0.8, self.threshold),
            // Chebyshev polynomial saturation based off the symmetrical saturation research - pretending to be tape
            SaturationModeEnum::CHEBYSHEV => return chebyshev_tape(compressed_sample_internal, 0.4),
            // Golden Cubic designed by Ardura
            SaturationModeEnum::GOLDENCUBIC => return golden_cubic(compressed_sample_internal, self.threshold),
            // Default to no saturation
            _ => return compressed_sample_internal,
        }

    }
}
