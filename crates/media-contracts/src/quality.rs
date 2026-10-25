//! Measurements of decoded output; detector events are technical observations.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QualityStream {
    pub codec: String,
    pub start_seconds: Option<f64>,
    pub duration_seconds: Option<f64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub average_rate: Option<(u32, u32)>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QualityInterval {
    pub start_seconds: f64,
    pub end_seconds: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputMeasurements {
    pub duration_seconds: f64,
    pub video: QualityStream,
    pub audio: Option<QualityStream>,
    pub black_intervals: Vec<QualityInterval>,
    pub frozen_intervals: Vec<QualityInterval>,
    pub integrated_lufs: Option<f64>,
    pub loudness_range_lu: Option<f64>,
    pub true_peak_dbfs: Option<f64>,
    pub video_frames_decoded: String,
    pub audio_blocks_analyzed: Option<String>,
    /// Full decoded output; this is not a perceptual lip-sync or aesthetic score.
    pub coverage: String,
}
