use crate::{config::SessionConfiguration, fingerprint::Feature};

use std::collections::{BinaryHeap, HashMap};
use uuid::Uuid;

pub struct DatabaseConfiguration {
    sample_rate: usize,
    window_stride: usize,

    chroma_bins_per_octave: usize,

    quantizer_bits_per_bin: usize,

    search_beam_count: usize,
    search_window_size: usize,
    search_nonmax_overlap: f32,
    search_length_penalty: u32,
    search_score_penalty: u32,
}

impl From<&SessionConfiguration> for DatabaseConfiguration {
    fn from(value: &SessionConfiguration) -> Self {
        Self {
            sample_rate: value.sample_rate,
            window_stride: value.window_stride,

            chroma_bins_per_octave: value.chroma_bins_per_octave,
            quantizer_bits_per_bin: value.quantizer_bits_per_bin,

            search_beam_count: value.search_beam_count,
            search_window_size: value.search_window_size,
            search_nonmax_overlap: value.search_nonmax_overlap,
            search_length_penalty: value.search_length_penalty,
            search_score_penalty: value.search_score_penalty
        }
    }
}

struct QueryResult {
    score: f32, 
    uuid: Uuid, 
    start: usize, 
    end: usize,
    query_start: usize,
}

impl<'a> From<Beam<'a>> for QueryResult {
    fn from(beam: Beam<'a>) -> Self {
        Self {
            score: beam.score(), 
            uuid: *beam.src.0, 
            start: beam.start(), 
            end: beam.end(),
            query_start: beam.query_start
        }
    }
}

struct Beam<'a> {
    src: (&'a Uuid, &'a [Feature]),

    path: Vec<usize>,
    score_frac: (u32, u32),
    query_start: usize,
}

impl<'a> PartialEq for Beam<'a> {
    fn eq(&self, other: &Self) -> bool {
        self.score_frac.0 * other.score_frac.1 == other.score_frac.0 * self.score_frac.1
    }
}
impl<'a> Eq for Beam<'a> {}
impl<'a> PartialOrd for Beam<'a> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl<'a> Ord for Beam<'a> {
    // we want the first item to be the highest scoring (worst) one
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let selfscore = self.score_frac.0 * other.score_frac.1;
        let otherscore = other.score_frac.0 * self.score_frac.1;
        selfscore.cmp(&otherscore)
    }
}

impl<'a> Beam<'a> {
    pub fn score(&self) -> f32 {
        self.score_frac.0 as f32 / self.score_frac.1 as f32
    }

    pub fn start(&self) -> usize { *self.path.first().unwrap() }
    pub fn end(&self) -> usize { *self.path.last().unwrap() }

    pub fn push(&mut self, distance: u32, index: usize) {
        self.path.push(index);
        self.score_frac.0 += distance;
        self.score_frac.1 += 1;
    }
}

struct Database {
    cfg: DatabaseConfiguration,
    database: HashMap<Uuid, Vec<Feature>>
}

struct Query<'a> {
    beams: BinaryHeap<Beam<'a>>,
    head: usize,
    database: &'a Database,
}

impl Database {
    pub fn register(&mut self, features: Vec<Feature>) -> Uuid {
        let key = Uuid::new_v4();
        self.database.insert(key.clone(), features);
        key
    }

    pub fn new_query<'a>(&'a self) -> Query<'a> {
        Query { beams: BinaryHeap::with_capacity(self.cfg.search_beam_count), database: self, head: 0 }
    }
}

impl<'a> Query<'a> {
    pub fn update(&mut self, new_feature: Feature) {
        let cfg = &self.database.cfg;

        // search through exisiting beams
        let mut new_beams = BinaryHeap::new();

        // timewarp existing beams, update score
        while let Some(mut beam) = self.beams.pop() {
            let head = beam.end();

            // search for new head based on feature
            let (_, features) = &beam.src;

            let start = head+1;
            let end = (start+cfg.search_window_size).min(features.len());

            let min = features[start..end]
                .iter()
                .enumerate()
                .map(|(offset, cand_feature)| 
                    (offset, cand_feature.distance(&new_feature)))
                .min_by_key(|item| item.1);

            if let Some((offset, distance)) = min {
                beam.push(distance, start+offset);

                //worst_mean_distance = worst_mean_distance.max(beam.mean_distance());

                new_beams.push(beam);
            }
        }

        // search to seed new beams
        //let mut cand_beams = BinaryHeap::new();

        for (uuid, features) in &self.database.database {
            for (head, feature) in features.iter().enumerate() {
                let distance = new_feature.distance(feature);

                let beam = Beam {
                    src: (uuid, features),
                    path: vec![head],
                    score_frac: (cfg.search_score_penalty + distance,
                        cfg.search_length_penalty + 1),
                    query_start: self.head
                };

                if new_beams.len() < cfg.search_beam_count {
                    new_beams.push(beam);
                } else if beam.cmp(new_beams.peek().unwrap()) == std::cmp::Ordering::Less {
                    // add this new beam if it's better than the worst beam in the set.
                    new_beams.pop();
                    new_beams.push(beam);
                }
            }
        }

        self.head += 1;
        self.beams = new_beams;
    }

    pub fn finalize(self) -> Vec<QueryResult> {
        let mut beams = self.beams.into_sorted_vec();
        beams.reverse();

        let mut results = Vec::new();

        // we need to merge contiguous beams

        while let Some(beam) = beams.pop() {
            // invalidate overlapping beams
            beams.retain(|other_beam| {
                let intersction_start = beam.start().max(other_beam.start());
                let intersection_end = beam.end().min(other_beam.end());
                let intersection = (intersection_end as isize - intersction_start as isize).max(0);
                let union = other_beam.end() - other_beam.start();

                let overlap = if union != 0 { intersection as f32 / union as f32 } else { 0.0 };
                
                overlap < self.database.cfg.search_nonmax_overlap
            });

            results.push(beam.into());
        }

        results
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::SessionConfiguration, fingerprint::FeatureExtractor};
    use std::path::Path;
    use std::time::Instant;

    fn resample(samples: &[i16], from_rate: u32, to_rate: u32) -> Vec<i16> {
        if from_rate == to_rate {
            return samples.to_vec();
        }

        let ratio = from_rate as f64 / to_rate as f64;
        let output_len = (samples.len() as f64 / ratio).ceil() as usize;
        let mut output = Vec::with_capacity(output_len);

        for i in 0..output_len {
            let src_pos = i as f64 * ratio;
            let src_idx = src_pos.floor() as usize;
            let frac = src_pos - src_idx as f64;

            if src_idx + 1 < samples.len() {
                // Linear interpolation
                let sample1 = samples[src_idx] as f64;
                let sample2 = samples[src_idx + 1] as f64;
                let interpolated = sample1 + (sample2 - sample1) * frac;
                output.push(interpolated.round() as i16);
            } else if src_idx < samples.len() {
                output.push(samples[src_idx]);
            }
        }

        output
    }

    fn load_wav_features(path: &str, extractor: &FeatureExtractor, target_sample_rate: u32) -> Vec<Feature> {
        let mut reader = hound::WavReader::open(path)
            .expect(&format!("Failed to open {}", path));

        let spec = reader.spec();
        let original_sample_rate = spec.sample_rate;
        let channels = spec.channels as usize;

        let samples: Vec<i16> = reader.samples::<i16>()
            .map(|s| s.expect("Failed to read sample"))
            .collect();

        // downmix channels if necessary
        let samples: Vec<i16> = if channels > 1 {
            samples.chunks(channels)
                .map(|c| (c.iter().map(|&v| v as i32).sum::<i32>() / channels as i32) as i16)
                .collect()
        } else { samples };

        // Resample if necessary
        let resampled = if original_sample_rate != target_sample_rate {
            println!("  Resampling from {} Hz to {} Hz", original_sample_rate, target_sample_rate);
            resample(&samples, original_sample_rate, target_sample_rate)
        } else {
            samples
        };

        extractor.features(resampled)
    }

    #[test]
    fn test_search_query_piano() {
        // Create configuration and feature extractor
        let config = SessionConfiguration::default();
        let (extractor_cfg, db_cfg) = config.into_child_configs();
        let extractor: FeatureExtractor = extractor_cfg.into();

        // Create database and populate with files from ../key directory
        let mut database = Database {
            cfg: db_cfg,
            database: HashMap::new(),
        };

        // Load all WAV files from the key directory
        let key_dir = Path::new("../key");
        let key_files = vec![
            "mary_had_a_lamb.wav",
            "hot_cross.wav",
            "chord_pos.wav",
            "chord_neg.wav",
            "summer.wav",
            "fake_violins.wav"
        ];
        let mut registry = HashMap::new();

        let target_sample_rate = config.sample_rate as u32;

        for file in key_files {
            let path = key_dir.join(file);
            if path.exists() {
                println!("Loading key file: {}", file);
                let features = load_wav_features(path.to_str().unwrap(), &extractor, target_sample_rate);
                println!("  Extracted {} features", features.len());
                let uuid = database.register(features);
                registry.insert(uuid, path);
            }
        }

        assert!(!database.database.is_empty(), "Database should contain at least one entry");

        // Load query file and extract features
        let query_path = "../query_summer.wav";
        println!("\nLoading query file: {}", query_path);
        let query_features = load_wav_features(query_path, &extractor, target_sample_rate);
        println!("Query has {} features", query_features.len());

        // Create query and process all features
        let start = Instant::now();
        let mut query = database.new_query();
        for (i, feature) in query_features.iter().enumerate() {
            query.update(*feature);
        }
        // Finalize and get results
        let results = query.finalize();
        let end = Instant::now();

        println!("\nFound {} matches in {:?}:", results.len(), end - start);
        for (i, result) in results.iter().enumerate() {
            println!("  Match {}: score={}, path={:?} from={}, {}s - {}s",
                i + 1, 
                result.score,
                registry.get(&result.uuid).unwrap(), 
                result.query_start as f32 * config.stride_dt(),
                result.start as f32 * config.stride_dt(), 
                (result.end + 1) as f32 * config.stride_dt());
        }

        // Verify we got at least one result
        assert!(!results.is_empty(), "Should find at least one match");
    }

    #[test]
    fn test_feature_distance() {
        let f1 = Feature::from(0b1010u64);
        let f2 = Feature::from(0b1100u64);

        // XOR: 0b1010 ^ 0b1100 = 0b0110 (2 bits set)
        assert_eq!(f1.distance(&f2), 2);

        let f3 = Feature::from(0u64);
        let f4 = Feature::from(u64::MAX);

        // All 64 bits are different
        assert_eq!(f3.distance(&f4), 64);
    }
}