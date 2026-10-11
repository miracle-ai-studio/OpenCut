//! Internal, persisted execution snapshots for schema 3 jobs.
use opencut_contracts::{AssetLockManifest, Time, TimeRange, project_v3::ProjectSnapshotV3};
use opencut_media_contracts::{Container, FrameAlignment, VideoCodec};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreparedSequenceTranscribe {
    pub snapshot: ProjectSnapshotV3,
    pub sequence_id: String,
    pub language: Option<String>,
    pub asset_lock: AssetLockManifest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreparedV3Render {
    pub snapshot: ProjectSnapshotV3,
    pub sequence_id: String,
    pub container: Container,
    pub video_codec: VideoCodec,
    pub asset_lock: AssetLockManifest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreparedV3Frame {
    pub snapshot: ProjectSnapshotV3,
    pub sequence_id: String,
    pub requested_time: Time,
    pub alignment: FrameAlignment,
    pub asset_lock: AssetLockManifest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreparedV3Range {
    pub snapshot: ProjectSnapshotV3,
    pub sequence_id: String,
    pub requested_range: TimeRange,
    pub alignment: FrameAlignment,
    pub asset_lock: AssetLockManifest,
}
