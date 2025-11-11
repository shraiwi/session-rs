use crate::{config::SessionConfiguration, fingerprint::Feature};

use std::{cmp::Ordering, collections::{BinaryHeap, HashMap, hash_map::Entry::{Occupied, Vacant}}};
use uuid::Uuid;
use serde::{Serialize, Deserialize};

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

pub struct QueryResult {
    pub uuid: Uuid,
    pub score: f32, 
    pub key_start: usize, 
    pub key_end: usize,
    pub query_start: usize,
}

struct Fraction { n: u32, d: u32 }

impl Fraction {
    pub fn to_f32(&self) -> f32 { self.n as f32 / self.d as f32 }
}

impl PartialEq for Fraction {
    fn eq(&self, other: &Self) -> bool {
        self.n * other.d == other.n * self.d
    }
}
impl Eq for Fraction {}
impl PartialOrd for Fraction {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Fraction {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let selfscore = self.n * other.d;
        let otherscore = other.n * self.d;
        selfscore.cmp(&otherscore)
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Beam {
    query_start: usize,
    path: Vec<usize>,
}

impl Beam {
    fn key_start(&self) -> usize { *self.path.first().unwrap() }
    fn key_end(&self) -> usize { *self.path.last().unwrap() }
}


pub struct Query<'a> {
    database: &'a Database,
    head: usize,
    song_beams: Vec<(&'a Uuid, &'a [Feature], Vec<(Fraction, Beam)>)>,
}

pub struct Database {
    cfg: DatabaseConfiguration,
    database: HashMap<Uuid, Vec<Feature>>
}


impl Database {
    pub fn insert(&mut self, key: Uuid, features: Vec<Feature>) {
        self.database.insert(key, features);
    }

    pub fn new_query<'a>(&'a self) -> Query<'a> {
        let beams = self.database
            .iter()
            .map(|(uuid, features)|
                (uuid, features.as_slice(), Vec::with_capacity(self.cfg.search_beam_count)))
            .collect();

        Query { song_beams: beams, database: self, head: 0 }
    }
}

impl From<DatabaseConfiguration> for Database {
    fn from(cfg: DatabaseConfiguration) -> Self {
        Self { cfg, database: HashMap::new() }
    }
}

impl<'a> Query<'a> {

    pub fn update(&mut self, new_feature: Feature) {

        // allows us to lazily allocate a new beam
        #[derive(PartialEq, Eq, PartialOrd, Ord)]
        enum Candidate {
            Existing(Beam),
            Seed(usize)
        }

        impl Candidate {
            fn to_beam(self, query_start: usize) -> Beam {
                match self {
                    Self::Existing(beam) => beam,
                    Self::Seed(key_start) => Beam { query_start, path: vec![key_start] }
                }
            }
        }

        let cfg = &self.database.cfg;

        /*
        for each song, timewarp existing beams and seed new ones using the new feature.
        perform automatic merging/matching  of songs using end/start tables
        */

        for (uuid, features, beams) in self.song_beams.iter_mut() {

            // seed recombination table
            let scores: Vec<u32> = features
                .iter()
                .map(|key_feature| new_feature.distance(key_feature))
                .collect();

            let mut recomb_table: HashMap<usize, (Fraction, Candidate)> = HashMap::new();

            // combine with existing beams
            for (mut score, mut beam) in beams.drain(..) { // get beam

                // extend beam

                let head = beam.key_end();

                let start = head+1;
                let end = (start+cfg.search_window_size).min(scores.len());

                let min = scores[start..end]
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, &d)| d);

                if let Some((offset, distance)) = min {
                    beam.path.push(start + offset);
                    score.n += distance;
                    score.d += 1;
                }
                
                let entry = recomb_table.entry(beam.key_end());

                match entry {
                    Vacant(entry) => { entry.insert((score, Candidate::Existing(beam))); }
                    Occupied(entry) => { // presumably the entry is another competing beam
                        let (other_score, other_beam) = entry.into_mut();

                        if score.cmp(other_score) == Ordering::Less { // if this beam is stronger, insert
                            *other_score = score;
                            *other_beam = Candidate::Existing(beam);
                        }
                    }
                }
            }

            // seed new beams
            for (key_start, distance) in scores.into_iter().enumerate() {
                let score = Fraction { n: cfg.search_score_penalty + distance, d: cfg.search_length_penalty + 1 };

                let entry = recomb_table.entry(key_start);

                match entry {
                    Vacant(entry) => { entry.insert((score, Candidate::Seed(key_start))); }
                    Occupied(entry) => { // presumably the entry is another competing beam
                        let (other_score, other_beam) = entry.into_mut();

                        if score.cmp(other_score) == Ordering::Less { // if this beam is stronger, insert
                            *other_score = score;
                            *other_beam = Candidate::Seed(key_start);
                        }
                    }
                }
            }

            let mut heap: BinaryHeap<_> = recomb_table
                .into_values()
                .collect();

            // trim heap size, removing high scoring elements until size is OK.
            while heap.len() > cfg.search_beam_count { heap.pop(); }

            // convert hashmap into maxheap
            *beams = heap
                .drain()
                .map(|(score, cand)| (score, cand.to_beam(self.head)))
                .collect();
        }
        
        self.head += 1;
    }

    pub fn finalize(self) -> Vec<QueryResult> {
        // get minheap
        let mut heap: BinaryHeap<(Fraction, &Uuid, Beam)> = self.song_beams
            .into_iter()
            .flat_map(|(uuid, _, beams)| beams
                .into_iter()
                .map(move |(score, beam)| (score, uuid, beam)))
            .collect();

        /*


        let mut beams = self.song_beams.into_sorted_vec();
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

        results*/

        let beams: Vec<_> = heap
            .into_sorted_vec()
            .into_iter()
            .map(|(score, uuid, beam)| QueryResult { 
                uuid: *uuid, 
                score: score.to_f32(), 
                key_start: beam.key_start(),
                key_end: beam.key_end(),
                query_start: beam.query_start
            })
            .collect();
        //beams.reverse();

        beams
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::SessionConfiguration, fingerprint::FeatureExtractor};
    use std::path::Path;
    use std::time::Instant;
    use url::Url;

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

        let converted: Vec<f32> = resampled.into_iter().map(|f| f as f32 / i16::MAX as f32).collect();

        extractor.features(&converted)
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
                let uuid = Uuid::new_v4();
                database.insert(uuid, features);
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
            let path = registry.get(&result.uuid).unwrap().canonicalize().unwrap();
            println!("  Match {}: score={}, from={:.2}s, to={}#t={:.2},{:.2} ",
                i + 1, 
                result.score,
                result.query_start as f32 * config.stride_dt(),
                Url::from_file_path(path).unwrap(), 
                result.key_start as f32 * config.stride_dt(), 
                (result.key_end + 1) as f32 * config.stride_dt());
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