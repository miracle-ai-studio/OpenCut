//! Internal, persisted execution snapshots for schema 3 jobs.
use opencut_contracts::{
    AssetLockManifest, Time, TimeRange, historical_visual::HistoricalVisualSnapshot,
};
use opencut_media_contracts::{Container, FrameAlignment, VideoCodec};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreparedSequenceTranscribe {
    pub snapshot: HistoricalVisualSnapshot,
    pub sequence_id: String,
    pub language: Option<String>,
    pub asset_lock: AssetLockManifest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoricalVisualPreparedRender {
    pub snapshot: HistoricalVisualSnapshot,
    pub sequence_id: String,
    pub container: Container,
    pub video_codec: VideoCodec,
    pub asset_lock: AssetLockManifest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreparedHistoricalVisualFrame {
    pub snapshot: HistoricalVisualSnapshot,
    pub sequence_id: String,
    pub requested_time: Time,
    pub alignment: FrameAlignment,
    pub asset_lock: AssetLockManifest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoricalVisualPreparedRange {
    pub snapshot: HistoricalVisualSnapshot,
    pub sequence_id: String,
    pub requested_range: TimeRange,
    pub alignment: FrameAlignment,
    pub asset_lock: AssetLockManifest,
}
