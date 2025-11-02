extern crate nalgebra as na;
use std::sync::Arc;

use realfft::{num_complex::ComplexFloat, RealFftPlanner, RealToComplex};

use na::{DMatrix};

use crate::config::SessionConfiguration;

pub struct FeatureExtractorConfiguration {
    sample_rate: usize,
    window_size: usize,
    window_stride: usize,

    chroma_n_octaves: usize, 
    chroma_bins_per_octave: usize,
    chroma_f_ref: f32,
    chroma_q_factor: f32,

    quantizer_min_energy: f32,
    quantizer_bits_per_bin: usize,
    quantizer_topk: usize,
}

#[derive(Clone, Copy)]
pub struct Feature(u64);

impl Feature {
    pub fn distance(&self, other: &Self) -> u32 {
        (self.0 ^ other.0).count_ones()
    }
}
impl AsMut<u64> for Feature {
    fn as_mut(&mut self) -> &mut u64 { &mut self.0 } }
impl AsRef<u64> for Feature { 
    fn as_ref(&self) -> &u64 { &self.0 } }
impl From<u64> for Feature {
    fn from(value: u64) -> Self { Self(value) } }

impl From<&SessionConfiguration> for FeatureExtractorConfiguration {
    fn from(value: &SessionConfiguration) -> Self {
        Self {
            sample_rate: value.sample_rate,
            window_size: value.window_size,
            window_stride: value.window_stride,

            chroma_n_octaves: value.chroma_n_octaves,
            chroma_bins_per_octave: value.chroma_bins_per_octave,
            chroma_f_ref: value.chroma_f_ref,
            chroma_q_factor: value.chroma_q_factor,

            quantizer_min_energy: value.quantizer_min_energy,
            quantizer_bits_per_bin: value.quantizer_bits_per_bin,
            quantizer_topk: value.quantizer_topk,
        }
    }
}


pub struct FeatureExtractor {
    cfg: FeatureExtractorConfiguration,
    chroma: DMatrix<f32>,
    fft: Arc<dyn RealToComplex<f32>>,
    window: Vec<f32>,
}

impl From<FeatureExtractorConfiguration> for FeatureExtractor {
    fn from(cfg: FeatureExtractorConfiguration) -> Self {
        let chroma = Self::chroma_matrix(&cfg);

        let mut fft_planner = RealFftPlanner::new();
        let fft = fft_planner.plan_fft_forward(cfg.window_size);

        let omega = std::f32::consts::TAU / (cfg.window_size - 1) as f32;
        let window: Vec<f32> = (0..cfg.window_size)
            .map(|i| 0.5 - 0.5 * (i as f32 * omega).cos())
            .collect();

        Self { cfg, chroma, fft, window }
    }
}

impl FeatureExtractor {

    fn a_curve(f: f32) -> f32 {
        const C0_SQ: f32 = 20.6 * 20.6;
        const C1_SQ: f32 = 107.7 * 107.7;
        const C2_SQ: f32 = 737.9 * 737.9;
        const C3: f32 = 12194.0;
        const C3_SQ: f32 = C3 * C3;

        if f <= 0.0 { return 0.0 }

        let f_sq = f.powi(2);

        let num_log = 2.0 * C3.ln() + 4.0 * f.ln();
        let denom_log = (f_sq + C0_SQ).ln()
            + 0.5 * (f_sq + C1_SQ).ln()
            + 0.5 * (f_sq + C2_SQ).ln()
            + (f_sq + C3_SQ).ln();
        
        (num_log - denom_log).exp()
    }

    fn chroma_matrix(cfg: &FeatureExtractorConfiguration) -> DMatrix<f32> {
        /*
        window_size/2+1 x chroma_bins_per_octave
        */

        let nrows = cfg.window_size / 2 + 1;
        let ncols = cfg.chroma_bins_per_octave;
        let bin_step = (cfg.chroma_bins_per_octave as f32).recip();

        DMatrix::from_fn(nrows, ncols, |fft_index, bin_index| {
            // row is the sample index within FFT
            // col is output bin (center of filter)
            let row_freq = (cfg.sample_rate as f32) * (fft_index as f32) / (cfg.window_size as f32);

            let bin_factor: f32 = (0..cfg.chroma_n_octaves)
                .map(| octave | {
                    let octave_frac = octave as f32 + bin_index as f32 * bin_step;
                    let tone_freq = octave_frac.exp2() * cfg.chroma_f_ref;
                    
                    // sigma = target tone center / q
                    // z = (tone - fft tone) / sigma
                    //   = (tone - fft tone) * q / target tone
                    let z = (tone_freq - row_freq) * cfg.chroma_q_factor / tone_freq;

                    (z.powi(2) * -0.5).exp()
                })
                .sum();
            
            bin_factor * Self::a_curve(row_freq)
        })
    }

    pub fn features(&self, audio: Vec<i16>) -> Vec<Feature> {
        let cfg = &self.cfg;

        // build spectogram of audio

        let mut input = self.fft.make_input_vec();
        let mut output = self.fft.make_output_vec();
        let mut scratch = self.fft.make_scratch_vec();

        let audio: Vec<f32> = audio
            .iter()
            .map(|s| (*s as f32) * 2f32.powi(-15) )
            .collect();

        let windows = audio
            .windows(cfg.window_size)
            .step_by(cfg.window_stride);

        let mut spectrogram: DMatrix<f32> = DMatrix::zeros(windows.len(), output.len());

        for (chunk_index, chunk) in windows.enumerate() {
            input[..chunk.len()]
                .iter_mut()
                .enumerate()
                .for_each(|(i, s)| *s = chunk[i] * self.window[i]);
            input[chunk.len()..].fill(0.0);

            let _ = self.fft.process_with_scratch(&mut input, &mut output, &mut scratch);
            let normalizing_factor = (cfg.window_size as f32).sqrt().recip();

            for i in 0..output.len() {
                spectrogram[(chunk_index, i)] = output[i].abs() * normalizing_factor;
            }
        }

        // downproject to chroma vectors
        let chroma_vectors = spectrogram * &self.chroma;
        
        // quantize chroma vectors
        
        let mut features = Vec::with_capacity(chroma_vectors.shape().0);
        
        let mut sorted_chroma = Vec::with_capacity(chroma_vectors.shape().1);
        for chroma_vector in chroma_vectors.row_iter() {
            // need to implement median filtering?

            sorted_chroma.extend(chroma_vector.iter().enumerate().map(|(i, &v)| (v, i)));

            sorted_chroma.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
            // sorted chroma is now in ascended order. the percentile of the element
            // at index i is its position in this new array
            let feature = sorted_chroma.drain(sorted_chroma.len()-cfg.quantizer_topk..)
                .enumerate()
                .map(|(new_index, (_, old_index))| {
                    let bin = new_index * (cfg.quantizer_bits_per_bin + 1) / cfg.quantizer_topk;
                    let tempcode = (1u64 << bin) - 1;
                    tempcode << (old_index * cfg.quantizer_bits_per_bin)
                })
                .reduce(|a, b| a | b)
                .unwrap_or(0)
                .into();

            features.push(feature);

            sorted_chroma.clear();
        }

        features
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn test_fingerprint_downsamp() {
        // Read the WAV file
        let mut reader = hound::WavReader::open("../test_downsamp.wav")
            .expect("Failed to open test_downsamp.wav");

        let spec = reader.spec();
        println!("WAV spec: {:?}", spec);

        // Read samples as i16
        let samples: Vec<i16> = reader.samples::<i16>()
            .map(|s| s.expect("Failed to read sample"))
            .collect();

        println!("Read {} samples", samples.len());

        // Create feature extractor with default configuration
        let (config, _) = SessionConfiguration::default().into_child_configs();
        let extractor: FeatureExtractor = config.into();

        // Extract features
        let features = extractor.features(samples);

        println!("Extracted {} features", features.len());

        // Write features to text file
        let mut file = File::create("test_downsamp_features.txt")
            .expect("Failed to create output file");

        for (i, feature) in features.iter().enumerate() {
            writeln!(file, "{}: 0x{:064b}", i, feature.as_ref())
                .expect("Failed to write to file");
        }

        println!("Features written to test_downsamp_features.txt");

        // Verify we got some features
        assert!(!features.is_empty(), "Should extract at least one feature");
    }
}