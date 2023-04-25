use nih_plug::prelude::*;

use duro_compressor::Gain;

fn main() {
    nih_export_standalone::<Gain>();
}
