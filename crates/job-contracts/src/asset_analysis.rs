//! Native V4 asset analysis; old snapshot-bearing analysis protocols stay frozen.
use opencut_contracts::LockedAsset;
use opencut_media_contracts::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
pub const KIND: &str = "analysis.asset.v4";
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubmitAssetAnalysis {
    pub project_id: String,
    pub revision: String,
    pub asset_id: String,
    pub idempotency_key: String,
    pub options: AssetAnalysisOptions,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AssetAnalysisOptions {
    Silence {
        noise_db: i16,
        min_duration_ms: u32,
    },
    Shots {
        threshold: f32,
    },
    Waveform {
        buckets: u16,
    },
    Transcribe {
        language: Option<String>,
        word_timestamps: bool,
    },
    SpeakerTurns {},
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    content = "result",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum AssetAnalysisData {
    Silence(SilenceResult),
    Shots(ShotsResult),
    Waveform(WaveformResult),
    Transcribe(TranscriptionResult),
    SpeakerTurns(SpeakerTurnsResult),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssetAnalysisResult {
    pub schema_version: u32,
    pub project_id: String,
    pub revision: String,
    pub asset_id: String,
    pub asset_version: LockedAsset,
    pub options: AssetAnalysisOptions,
    pub data: AssetAnalysisData,
}
