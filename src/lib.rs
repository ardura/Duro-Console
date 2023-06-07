mod ui_knob;
use atomic_float::AtomicF32;
use duro_process::{Console, SaturationModeEnum};
use nih_plug::{prelude::*};
use nih_plug_egui::{create_egui_editor, egui::{self, mutex::{Mutex}, plot::{Line, PlotPoints, HLine}, Color32, Stroke, Rect, Rounding, pos2}, widgets, EguiState};
use crate::ui_knob::{ArcKnob, TextSlider};
use std::{sync::{Arc, atomic::{Ordering}}, fmt::format};
mod duro_process;

/**************************************************
 * Duro Console by Ardura
 * 
 * Build with: cargo xtask bundle duro_console
 * ************************************************/

// GUI Colors
 const LIGHTTEAL: Color32 = Color32::from_rgb(142, 202, 230);
 const TEAL: Color32 = Color32::from_rgb(33, 158, 188);
 const DARKTEAL: Color32 = Color32::from_rgb(2, 48, 71);
 const MACARONI: Color32 = Color32::from_rgb(255, 183, 3);
 const ORANGE: Color32 = Color32::from_rgb(251, 133, 0);

/// The time it takes for the peak meter to decay by 12 dB after switching to complete silence.
const PEAK_METER_DECAY_MS: f64 = 100.0;

pub struct Gain {
    params: Arc<GainParams>,

    // normalize the peak meter's response based on the sample rate with this
    out_meter_decay_weight: f32,

    // Console class
    console: Console,

    // The current data for the different meters
    out_meter: Arc<AtomicF32>,
    in_meter: Arc<AtomicF32>,
}

#[derive(Params)]
struct GainParams {
    /// The editor state, saved together with the parameter state so the custom scaling can be
    /// restored.
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    #[id = "free_gain"]
    pub free_gain: FloatParam,

    #[id = "threshold"]
    pub threshold: FloatParam,

    #[id = "drive"]
    pub drive: FloatParam,

    #[id = "type"]
    pub sat_type: EnumParam<duro_process::SaturationModeEnum>,

    #[id = "console_type"]
    pub console_type: EnumParam<duro_process::ConsoleMode>,

    #[id = "output_gain"]
    pub output_gain: FloatParam,

    #[id = "dry_wet"]
    pub dry_wet: FloatParam,
}

impl Default for Gain {
    fn default() -> Self {
        Self {
            params: Arc::new(GainParams::default()),
            console:duro_process::Console::new(0.0,4,crate::duro_process::ConsoleMode::BYPASS,44100.0),
            out_meter_decay_weight: 1.0,
            out_meter: Arc::new(AtomicF32::new(util::MINUS_INFINITY_DB)),
            in_meter: Arc::new(AtomicF32::new(util::MINUS_INFINITY_DB)),
        }
    }
}

impl Default for GainParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(800, 160),

            // Input gain dB parameter (free as in unrestricted nums)
            free_gain: FloatParam::new(
                "Input Gain",
                util::db_to_gain(0.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(-12.0),
                    max: util::db_to_gain(12.0),
                    factor: FloatRange::gain_skew_factor(-12.0, 12.0),
                },
            )
            .with_smoother(SmoothingStyle::Logarithmic(50.0))
            .with_unit(" Input Gain")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(2))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),

            // Drive Parameter
            drive: FloatParam::new(
                "Drive",
                0.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 2.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(50.0))
            .with_unit(" % Drive")
            .with_value_to_string(formatters::v2s_f32_percentage(2))
            .with_string_to_value(formatters::s2v_f32_percentage()),

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
            .with_unit(" dB Threshold")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(2))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),

            // Output gain parameter
            output_gain: FloatParam::new(
                "Output Gain",
                util::db_to_gain(0.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(-12.0),
                    max: util::db_to_gain(12.0),
                    factor: FloatRange::gain_skew_factor(-12.0, 12.0),
                },
            )
            .with_smoother(SmoothingStyle::Logarithmic(50.0))
            .with_unit(" dB Output Gain")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(2))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),

            // Compressor Type parameter
            console_type: EnumParam::new("name",crate::duro_process::ConsoleMode::BYPASS),

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
            .with_unit("% Wet")
            .with_value_to_string(formatters::v2s_f32_percentage(2))
            .with_string_to_value(formatters::s2v_f32_percentage()),
        }
    }
}

impl Plugin for Gain {
    const NAME: &'static str = "Duro Console";
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
        let mut params = self.params.clone();
        let in_meter = self.in_meter.clone();
        let out_meter = self.out_meter.clone();
        create_egui_editor(
            self.params.editor_state.clone(),
            (),
            |_, _| {},
            move |egui_ctx, setter, _state| {
                egui::CentralPanel::default()
                    .show(egui_ctx, |ui| {
                        // Change colors - there's probably a better way to do this
                        let mut style_var = ui.style_mut().clone();
                        style_var.visuals.widgets.inactive.bg_fill = DARKTEAL;

                        // Assign default colors if user colors not set
                        style_var.visuals.widgets.inactive.fg_stroke.color = LIGHTTEAL;
                        style_var.visuals.widgets.noninteractive.fg_stroke.color = Color32::WHITE;
                        style_var.visuals.widgets.inactive.bg_stroke.color = ORANGE;
                        style_var.visuals.widgets.active.fg_stroke.color = Color32::LIGHT_RED;
                        style_var.visuals.widgets.active.bg_stroke.color = TEAL;
                        style_var.visuals.widgets.open.fg_stroke.color = MACARONI;
                        // Param fill
                        style_var.visuals.selection.bg_fill = TEAL;

                        style_var.visuals.widgets.noninteractive.bg_stroke.color = Color32::LIGHT_YELLOW;
                        style_var.visuals.widgets.noninteractive.bg_fill = Color32::RED;

                        // Trying to draw background as rect
                        ui.painter().rect_filled(Rect::EVERYTHING, Rounding::none(), DARKTEAL);

                        ui.set_style(style_var);

                        // GUI Structure
                        ui.horizontal(|ui| {
                            let mut gain_knob = ui_knob::ArcKnob::for_param(&params.free_gain, setter, 40.0);
                            gain_knob.set_center_size(10.0);
                            gain_knob.set_line_width(20.0);
                            gain_knob.set_center_to_line_space(0.0);
                            gain_knob.set_fill_color(Color32::GREEN);
                            gain_knob.set_line_color(Color32::LIGHT_GRAY);
                            ui.add(gain_knob);

                            let mut sat_type_knob = ui_knob::ArcKnob::for_param(&params.sat_type, setter, 40.0);
                            sat_type_knob.set_center_size(20.0);
                            sat_type_knob.set_line_width(5.0);
                            sat_type_knob.set_center_to_line_space(10.0);
                            sat_type_knob.set_fill_color(Color32::GOLD);
                            sat_type_knob.set_line_color(Color32::RED);
                            ui.add(sat_type_knob);

                            let mut threshold_knob = ui_knob::ArcKnob::for_param(&params.threshold, setter, 40.0);
                            threshold_knob.set_center_size(30.0);
                            threshold_knob.set_line_width(10.0);
                            threshold_knob.set_center_to_line_space(5.0);
                            threshold_knob.set_fill_color(Color32::WHITE);
                            threshold_knob.set_line_color(Color32::YELLOW);
                            ui.add(threshold_knob);

                            let mut drive_knob = ui_knob::ArcKnob::for_param(&params.drive, setter, 40.0);
                            drive_knob.set_center_size(5.0);
                            drive_knob.set_line_width(30.0);
                            drive_knob.set_center_to_line_space(20.0);
                            drive_knob.set_fill_color(MACARONI);
                            drive_knob.set_line_color(ORANGE);
                            ui.add(drive_knob);

                            let mut console_knob = ui_knob::ArcKnob::for_param(&params.console_type, setter, 40.0);
                            console_knob.set_center_size(10.0);
                            console_knob.set_line_width(15.0);
                            console_knob.set_center_to_line_space(24.0);
                            console_knob.set_fill_color(TEAL);
                            console_knob.set_line_color(Color32::LIGHT_GREEN);
                            ui.add(console_knob);

                            let mut output_knob = ui_knob::ArcKnob::for_param(&params.output_gain, setter, 40.0);
                            output_knob.set_center_size(10.0);
                            output_knob.set_line_width(10.0);
                            output_knob.set_center_to_line_space(10.0);
                            output_knob.set_fill_color(LIGHTTEAL);
                            output_knob.set_line_color(TEAL);
                            output_knob.use_outline(true);
                            ui.add(output_knob);

                            let mut dry_wet_knob = ui_knob::ArcKnob::for_param(&params.dry_wet, setter, 40.0);
                            dry_wet_knob.set_center_size(10.0);
                            dry_wet_knob.set_line_width(15.0);
                            dry_wet_knob.set_center_to_line_space(24.0);
                            dry_wet_knob.set_fill_color(TEAL);
                            dry_wet_knob.set_line_color(Color32::LIGHT_GREEN);
                            ui.add(dry_wet_knob);
                            }
                        )
                    });
                }
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

        //widgets::ParamEvent
        // Buffer level
        for channel_samples in buffer.iter_samples() {
            let mut out_amplitude = 0.0;
            let mut in_amplitude = 0.0;
            let mut processed_sample;
            let num_samples = channel_samples.len();

            let gain = util::gain_to_db(self.params.free_gain.smoothed.next());
            let mut num_gain: f32;
            let drive = self.params.drive.smoothed.next();
            let threshold = self.params.threshold.smoothed.next();
            let output_gain = self.params.output_gain.smoothed.next();
            let sat_type = self.params.sat_type.value();
            let console_type = self.params.console_type.value();
            let dry_wet = self.params.dry_wet.value();

            // Create the compressor object
            self.console.update_vals(threshold,drive,console_type,_context.transport().sample_rate);

            for sample in channel_samples {
                num_gain = gain;
                
                //nih_log!("{}  {}",gain,num_gain);
                
                *sample *= util::db_to_gain(num_gain);
                
                in_amplitude += *sample;

                // Perform processing on the sample
                processed_sample = self.console.duro_process(*sample, sat_type, console_type);

                // Calculate dry/wet mix (no compression but saturation possible)
                let wet_gain = dry_wet;
                let dry_gain = 1.0 - dry_wet;
                processed_sample = *sample * dry_gain + processed_sample * wet_gain;

                // get the output amplitude here
                processed_sample = processed_sample*output_gain;
                *sample = processed_sample;
                out_amplitude += processed_sample;
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
    const CLAP_ID: &'static str = "com.ardura.duro.console";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("A console with a combination of saturation algorithms");
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
    const VST3_CLASS_ID: [u8; 16] = *b"DuroConsoleAAAAA";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Distortion];
}

nih_export_clap!(Gain);
nih_export_vst3!(Gain);
