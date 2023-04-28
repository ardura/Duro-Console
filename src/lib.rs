use atomic_float::AtomicF32;
use duro_process::Compressor;
use nih_plug::{prelude::*};
use nih_plug_vizia::ViziaState;
use std::sync::Arc;
mod editor;
mod duro_process;

/**************************************************
 * Duro Compressor by Ardura
 * 
 * Build with: cargo xtask bundle duro_compressor
 * ************************************************/

/// The time it takes for the peak meter to decay by 12 dB after switching to complete silence.
const PEAK_METER_DECAY_MS: f64 = 100.0;

pub struct Gain {
    params: Arc<GainParams>,

    // normalize the peak meter's response based on the sample rate with this
    out_meter_decay_weight: f32,

    // Compressor class
    compressor: Compressor,

    // The current data for the different meters
    out_meter: Arc<AtomicF32>,
    in_meter: Arc<AtomicF32>,
    reduction_meter: Arc<AtomicF32>,
}

#[derive(Params)]
struct GainParams {
    /// The editor state, saved together with the parameter state so the custom scaling can be
    /// restored.
    #[persist = "editor-state"]
    editor_state: Arc<ViziaState>,

    #[id = "gain"]
    pub gain: FloatParam,

    #[id = "threshold"]
    pub threshold: FloatParam,

    #[id = "ratio"]
    pub ratio: IntParam,

    #[id = "type"]
    pub sat_type: EnumParam<duro_process::SaturationModeEnum>,

    #[id = "attack"]
    pub attack: IntParam,

    #[id = "release"]
    pub release: IntParam,

    #[id = "compressor_type"]
    pub compressor_type: EnumParam<duro_process::ThresholdMode>,

    #[id = "output_gain"]
    pub output_gain: FloatParam,

    #[id = "dry_wet"]
    pub dry_wet: FloatParam,
}

impl Default for Gain {
    fn default() -> Self {
        Self {
            params: Arc::new(GainParams::default()),
            compressor:duro_process::Compressor::new(0.0,4,0,0,crate::duro_process::ThresholdMode::LINEAR,0.0),
            out_meter_decay_weight: 1.0,
            out_meter: Arc::new(AtomicF32::new(util::MINUS_INFINITY_DB)),
            in_meter: Arc::new(AtomicF32::new(util::MINUS_INFINITY_DB)),
            reduction_meter: Arc::new(AtomicF32::new(util::MINUS_INFINITY_DB)),
        }
    }
}

impl Default for GainParams {
    fn default() -> Self {
        Self {
            editor_state: editor::default_state(),

            // Input gain parameter
            gain: FloatParam::new(
                "Input Gain",
                util::db_to_gain(0.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(-30.0),
                    max: util::db_to_gain(30.0),
                    factor: FloatRange::gain_skew_factor(-30.0, 30.0),
                },
            )
            .with_smoother(SmoothingStyle::Logarithmic(50.0))
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(2))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),

            // Compressor Ratio Parameter
            ratio: IntParam::new(
                "Ratio",
                4,
                IntRange::Linear {
                    min: 2,
                    max: 15,
                },
            )
            .with_smoother(SmoothingStyle::Linear(50.0)),
            //.with_unit(" dB")
            //.with_value_to_string(formatters::v2s_compression_ratio())
            //.with_string_to_value(formatters::s2v_compression_ratio()),

            // Threshold dB parameter
            threshold: FloatParam::new(
                "Threshold",
                util::db_to_gain(-6.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(-30.0),
                    max: util::db_to_gain(0.0),
                    factor: FloatRange::gain_skew_factor(-30.0, 0.0),
                },
            )
            .with_smoother(SmoothingStyle::Logarithmic(50.0))
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(2))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),

            // Output gain parameter
            output_gain: FloatParam::new(
                "Output Gain",
                util::db_to_gain(0.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(-30.0),
                    max: util::db_to_gain(30.0),
                    factor: FloatRange::gain_skew_factor(-30.0, 30.0),
                },
            )
            .with_smoother(SmoothingStyle::Logarithmic(50.0))
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(2))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),

            // Attack parameter
            attack: IntParam::new(
                "Attack",
                60,
                IntRange::Linear {
                    min: 1,
                    max: 2000,
                },
            )
            .with_smoother(SmoothingStyle::Logarithmic(50.0))
            .with_unit(" ms"),

            // Release parameter
            release: IntParam::new(
                "Release",
                30,
                IntRange::Linear {
                    min: 1,
                    max: 2000,
                },
            )
            .with_smoother(SmoothingStyle::Logarithmic(50.0))
            .with_unit(" ms"),

            // Compressor Type parameter
            compressor_type: EnumParam::new("name",crate::duro_process::ThresholdMode::LINEAR),

            // Saturation Type parameter
            sat_type: EnumParam::new("name",crate::duro_process::SaturationModeEnum::NONESAT),

            // Dry/Wet parameter
            dry_wet: FloatParam::new(
                "Dry/Wet",
                1.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 1.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(50.0))
            .with_value_to_string(formatters::v2s_f32_percentage(2))
            .with_string_to_value(formatters::s2v_f32_percentage()),
        }
    }
}

impl Plugin for Gain {
    const NAME: &'static str = "Duro Compressor";
    const VENDOR: &'static str = "Ardura";
    const URL: &'static str = "https://github.com/ardura";
    const EMAIL: &'static str = "azviscarra@gmail.com";

    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    // This looks like it's flexible for running the plugin in mono or stereo
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {main_input_channels: NonZeroU32::new(2), main_output_channels: NonZeroU32::new(2), ..AudioIOLayout::const_default()},
        AudioIOLayout {main_input_channels: NonZeroU32::new(1), main_output_channels: NonZeroU32::new(1), ..AudioIOLayout::const_default()},
    ];

    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        editor::create(
            self.params.clone(),
            self.in_meter.clone(),
            self.out_meter.clone(),
            self.reduction_meter.clone(),
            self.params.editor_state.clone(), 
        )
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        // After `PEAK_METER_DECAY_MS` milliseconds of pure silence, the peak meter's value should
        // have dropped by 12 dB
        self.out_meter_decay_weight = 0.25f64.powf((buffer_config.sample_rate as f64 * PEAK_METER_DECAY_MS / 1000.0).recip()) as f32;

        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {

        // Buffer level
        for channel_samples in buffer.iter_samples() {
            let mut out_amplitude = 0.0;
            let mut in_amplitude = 0.0;
            let mut reduction_amplitude = 0.0;
            let mut compressed_sample;
            let num_samples = channel_samples.len();

            let gain = self.params.gain.smoothed.next();
            let ratio = self.params.ratio.smoothed.next();
            let threshold = self.params.threshold.smoothed.next();
            let output_gain = self.params.output_gain.smoothed.next();
            let attack = self.params.attack.smoothed.next();
            let release = self.params.release.smoothed.next();
            let sat_type = self.params.sat_type.value();
            let compressor_type = self.params.compressor_type.value();
            let dry_wet = self.params.dry_wet.value();

            // Create the compressor object
            self.compressor.update_vals(threshold,ratio,attack,release,compressor_type,_context.transport().sample_rate);

            for sample in channel_samples {
                // get the input amplitude here
                if gain != 0.0 
                {
                    *sample *= gain;
                }
                
                in_amplitude += *sample;
   
                if in_amplitude != 0.0
                {
                    // Perform compression
                    compressed_sample = self.compressor.duro_compress(*sample, sat_type, compressor_type);
                }
                else 
                {
                    compressed_sample = *sample;
                }

                // Calculate dry/wet mix (no compression but saturation possible)
                let wet_gain = dry_wet;
                let dry_gain = 1.0 - dry_wet;
                compressed_sample = *sample * dry_gain + compressed_sample * wet_gain;

                //reduction_amplitude += compressed_sample - *sample;
                reduction_amplitude += *sample - compressed_sample;
               
                // get the output amplitude here
                compressed_sample = compressed_sample*output_gain;
                *sample = compressed_sample;
                out_amplitude += compressed_sample;
            }

            

            //win_dbg_logger::output_debug_string(format!("in_amplitude is {}, compressed_sample is {}, reduction_amplitude is {}, out_amplitude is {}\n", in_amplitude, compressed_sample, reduction_amplitude, out_amplitude).as_str());
            //win_dbg_logger::output_debug_string("--------------------------------------------\n");


            // To save resources, a plugin can (and probably should!) only perform expensive
            // calculations that are only displayed on the GUI while the GUI is open
            if self.params.editor_state.is_open() {
                // Input gain meter
                in_amplitude = (in_amplitude / num_samples as f32).abs();
                let current_in_meter = self.in_meter.load(std::sync::atomic::Ordering::Relaxed);
                let new_in_meter = if in_amplitude > current_in_meter {in_amplitude}                                else {current_in_meter * self.out_meter_decay_weight + in_amplitude * (1.0 - self.out_meter_decay_weight)};
                self.in_meter.store(new_in_meter, std::sync::atomic::Ordering::Relaxed);

                // Reduction gain meter
                reduction_amplitude = (reduction_amplitude / num_samples as f32).abs();
                let current_reduction_meter = self.reduction_meter.load(std::sync::atomic::Ordering::Relaxed);
                let new_reduction_meter = if reduction_amplitude > current_reduction_meter {reduction_amplitude}    else {current_reduction_meter * self.out_meter_decay_weight + reduction_amplitude * (1.0 - self.out_meter_decay_weight)};
                self.reduction_meter.store(new_reduction_meter, std::sync::atomic::Ordering::Relaxed);

                // Output gain meter
                out_amplitude = (out_amplitude / num_samples as f32).abs();
                let current_out_meter = self.out_meter.load(std::sync::atomic::Ordering::Relaxed);
                let new_out_meter = if out_amplitude > current_out_meter {out_amplitude}                            else {current_out_meter * self.out_meter_decay_weight + out_amplitude * (1.0 - self.out_meter_decay_weight)};
                self.out_meter.store(new_out_meter, std::sync::atomic::Ordering::Relaxed);
            }
        }

        ProcessStatus::Normal
    }

    const MIDI_INPUT: MidiConfig = MidiConfig::None;

    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;

    const HARD_REALTIME_ONLY: bool = false;

    fn task_executor(&self) -> TaskExecutor<Self> {
        // In the default implementation we can simply ignore the value
        Box::new(|_| ())
    }

    fn filter_state(_state: &mut PluginState) {}

    fn reset(&mut self) {}

    fn deactivate(&mut self) {}
}

impl ClapPlugin for Gain {
    const CLAP_ID: &'static str = "com.ardura.duro.compressor";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("A hard compressor with some flavor");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Mono,
        ClapFeature::Utility,
    ];
}

impl Vst3Plugin for Gain {
    const VST3_CLASS_ID: [u8; 16] = *b"DuroCompressorAa";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Dynamics];
}

nih_export_clap!(Gain);
nih_export_vst3!(Gain);
