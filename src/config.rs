use crate::{fingerprint::FeatureExtractorConfiguration, search::DatabaseConfiguration};


pub struct SessionConfiguration {
    // feature extractor
    pub sample_rate: usize,
    pub window_size: usize,
    pub window_stride: usize,

    pub chroma_n_octaves: usize, 
    pub chroma_bins_per_octave: usize,
    pub chroma_f_ref: f32,
    pub chroma_q_factor: f32,

    pub quantizer_min_energy: f32,
    pub quantizer_bits_per_bin: usize,
    pub quantizer_topk: usize,

    // search
    pub search_beam_count: usize,
    pub search_window_size: usize,
    pub search_nonmax_overlap: f32,
    pub search_length_penalty: u32,
    pub search_score_penalty: u32,
}

impl SessionConfiguration {
    pub fn into_child_configs(&self) -> (FeatureExtractorConfiguration, DatabaseConfiguration) {
        (self.into(), self.into())
    }

    pub fn stride_dt(&self) -> f32 { self.window_stride as f32 / self.sample_rate as f32  }
}

impl Default for SessionConfiguration {
    fn default() -> Self {
        Self {
            sample_rate: 11_500,
            window_size: 4096,
            window_stride: 2048,

            chroma_n_octaves: 8,
            chroma_bins_per_octave: 12,
            chroma_f_ref: 27.5,
            chroma_q_factor: 20.0,
            
            quantizer_min_energy: 0.05,
            quantizer_bits_per_bin: 5,
            quantizer_topk: 8,

            search_beam_count: 1000,
            search_window_size: 3,
            search_nonmax_overlap: 1.0,
            search_length_penalty: 3,
            search_score_penalty: 100,
        }
    }
}