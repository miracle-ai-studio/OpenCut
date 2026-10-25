//! Scene CFR output over the existing fixed-scene rendering and audio pipeline.
use super::media_frame::MediaFrameArtifactMetadata;
use opencut_contracts::{AssetLockManifest, FrameRate, Time, text_frame::FontLock};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const SCENE_VIDEO_MP4_KIND: &str = "render.project_scene_video.v4";

pub use super::media_video::{FixedVideoInput, MediaVideoExecutionSummary};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SceneVideoMp4Capability {
    pub schema_version: u32,
    pub available: bool,
    pub submit_tool: String,
    pub unavailable_reason: Option<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum SceneVideoProfile {
    #[serde(rename = "classic-v31-cfr-h264-aac-scene-v1")]
    ClassicV31CfrH264AacSceneV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SceneVideoIdentity {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub profile: SceneVideoProfile,
    pub width: u32,
    pub height: u32,
    pub fps: FrameRate,
    pub frame_count: String,
    /// Original scene end; CFR rounds up to the last complete output frame.
    pub timeline_duration: Time,
    pub video_duration: Time,
    pub sample_rate: u32,
    pub channels: u32,
    /// Pre-encoding shared Classic PCM. AAC framing is separately observed.
    pub audio_frame_count: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SceneVideoMp4Result {
    pub schema_version: u32,
    pub identity: SceneVideoIdentity,
    pub asset_lock: AssetLockManifest,
    pub font_lock: Vec<FontLock>,
    pub render_input_sha256: String,
    pub execution: MediaVideoExecutionSummary,
    pub artifact: MediaFrameArtifactMetadata,
}
