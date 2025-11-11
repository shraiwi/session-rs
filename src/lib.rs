pub mod fingerprint;
pub mod search;
pub mod config;
use wasm_bindgen::prelude::*;

pub use search::{Database, DatabaseConfiguration};
pub use fingerprint::{FeatureExtractor, FeatureExtractorConfiguration};
pub use config::SessionConfiguration;

#[wasm_bindgen]
pub fn resample(audio: &[f32], fs_in: u32, fs_out: u32) -> Vec<f32> {
    let resampled_len = audio.len() * fs_out as usize / fs_in as usize;

    let resampled = (0..resampled_len)
        .map(|i| {
            let ileft = i * audio.len() / resampled_len;
            let frac = (i * audio.len() % resampled_len) as f32 / resampled_len as f32;
            let left = audio[ileft];
            let right = audio[(ileft+1).min(audio.len()-1)];

            left * (1.0 - frac) + right * frac
        })
        .collect();

    resampled
}

#[wasm_bindgen]
pub struct Session {
    extractor: FeatureExtractor,
    db: Database,
    stride_dt: f32,
}

#[wasm_bindgen]
pub struct SessionQueryResult {
    uuid: String,

    #[wasm_bindgen(readonly)]
    pub score: f32,

    #[wasm_bindgen(js_name = keyStart, readonly)]
    pub key_start: f32,

    #[wasm_bindgen(js_name = keyEnd, readonly)]
    pub key_end: f32,

    #[wasm_bindgen(js_name = queryStart, readonly)]
    pub query_start: f32
}

#[wasm_bindgen]
impl SessionQueryResult {
    #[wasm_bindgen(getter)]
    pub fn uuid(&self) -> String {
        self.uuid.clone()
    }
}

#[wasm_bindgen]
impl Session {

    fn resample(audio: &[f32], fs_in: u32, fs_out: u32) -> Vec<f32> {
        let resampled_len = audio.len() * fs_out as usize / fs_in as usize;
        let resampled = (0..resampled_len)
            .map(|i| {
                let ileft = i * audio.len() / resampled_len;
                let frac = (i * audio.len() % resampled_len) as f32 / resampled_len as f32;
                let left = audio[ileft];
                let right = audio[(ileft+1).min(audio.len()-1)];

                left * (1.0 - frac) + right * frac
            })
            .collect();
        resampled
    }

    #[wasm_bindgen(constructor)]
    pub fn new(cfg: JsValue) -> Self {
        let cfg: SessionConfiguration = serde_wasm_bindgen::from_value(cfg)
            .unwrap_or_default();

        let stride_dt = cfg.stride_dt();

        let (extractor_cfg, db_cfg) = cfg.into_child_configs();

        Self {
            extractor: extractor_cfg.into(),
            db: db_cfg.into(),
            stride_dt
        }
    }

    #[wasm_bindgen]
    pub fn register(&mut self, uuid: String, audio: &[f32]) -> Result<(), JsError> {
        let uuid = uuid::Uuid::try_parse(&uuid)?;

        self.db.insert(uuid, self.extractor.features(audio));

        Ok(())
    }

    pub fn search(&mut self, audio: &[f32]) -> Vec<SessionQueryResult> {
        let features = self.extractor.features(audio);

        let mut q = self.db.new_query();

        for feature in features.into_iter() { q.update(feature); }

        q.finalize().into_iter()
            .map(|res| SessionQueryResult {
                uuid: res.uuid.to_string(),
                score: res.score,
                key_start: res.key_start as f32 * self.stride_dt,
                key_end: res.key_end as f32 * self.stride_dt,
                query_start: res.query_start as f32 * self.stride_dt,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

}
