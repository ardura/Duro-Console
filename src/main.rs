use nih_plug::prelude::*;

use duro_console::Gain;

use hound;

fn main() {
    // pipeline audio IUD impulses from 2007 used to generate the taps in a feedback delay network
    //let path: &str = "D:\\VST_ME\\New folder\\Duro_Compressor\\Pipelineaudio IUD Series Neve_1272\\Neve 1272\\Neve 1272 -20\\0 Neve 1272 -20.wav";
    let path: &str = "D:\\VST_ME\\New folder\\Duro_Compressor\\Pipelineaudio IUD Series Neve_1272\\Neve 1272\\Neve 1272 -50\\0 Neve 1272 -50.wav";
    //let path: &str = "D:\\VST_ME\\New folder\\Duro_Compressor\\Pipelineaudio IUD Series SSL_Logic_FX_G383\\SSL Logic FX G383\\SSL Logic FX G383 Mic Pre Flat\\-6 SSL Logic FX G383 Mic Pre Flat.wav";
    //let path: &str = "D:\\VST_ME\\New folder\\Duro_Compressor\\Pipelineaudio IUD Series dbx160\\dbx\\DBX 160SL Flat\\+6 DBX 160SL Flat.wav";
    //let path: &str = "D:\\VST_ME\\New folder\\Duro_Compressor\\Pipelineaudio IUD Series api\\api\\-3 API 512.wav";

    let mut reader = hound::WavReader::open(path).unwrap();
    let spec = reader.spec();
    let mut sampled_impulse: Vec<f32> = Vec::new();
    if spec.bits_per_sample == 24 {
        let read = reader.samples().collect::<Result<Vec<i32>, hound::Error>>();
        if let Ok(samples) = read {
            const I24_MAX: i32 = 2_i32.pow(23) - 1;
            sampled_impulse = samples
                .iter()
                .map(|val| *val as f32 / I24_MAX as f32)
                .collect();
        }
    }

    // Normalize impulse response
    let max_val = sampled_impulse.iter().fold(0.0, |max_val: f32, &val| max_val.max(val.abs()));
    sampled_impulse.iter_mut().for_each(|val| *val /= max_val);
    
    // Define feedback delay network parameters
    let delay_time = 0.001; // 1 ms
    let length = 512;
    
    // Allocate and initialize feedback delay network taps
    let mut taps = vec![0.0; length];
    for i in 0..length {
        let delay_samples = (i as f32 * delay_time * spec.sample_rate as f32) as usize;
        if delay_samples < sampled_impulse.len() {
            taps[i] = sampled_impulse[delay_samples];
        }
    }

    //let sampled_impulse: Vec<f32> = reader.samples::<f32>();//.map(|s| s.unwrap()).collect();
    println!("STARTING TAP");
    println!("{:?}", taps);
    println!("ENDING TAP");
    nih_export_standalone::<Gain>();
}
