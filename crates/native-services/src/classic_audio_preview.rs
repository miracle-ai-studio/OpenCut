//! Long-lived, bounded unmastered PCM producer. Playback/mastering belongs to
//! the local consumer; this adapter does not publish a WAV or allocate a scene.
mod producer;
mod protocol;
pub use producer::ClassicAudioPreviewProducer;
use serde_json::Value;
use std::{path::PathBuf, time::Duration};

pub fn new_identity() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub struct ClassicAudioPreviewProcessConfig {
    pub node: PathBuf,
    pub runner: PathBuf,
    pub chromium: PathBuf,
    pub preparation_deadline: Duration,
    pub max_pcm_bytes: u64,
    pub max_encoded_bytes: u64,
    pub max_rss_bytes: u64,
    pub strict_cgroup: Option<PathBuf>,
}
pub struct ClassicAudioPreviewPcmBlock {
    pub generation: u64,
    pub overlay_epoch: u64,
    pub start_sample: u64,
    pub frame_count: u32,
    pub channels: [Vec<f32>; 2],
    pub receipt: Value,
}
