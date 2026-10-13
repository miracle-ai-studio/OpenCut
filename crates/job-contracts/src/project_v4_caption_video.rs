//! Additive fixed-scene CFR output. Rendering stays on the V4 shared backend.
use super::project_v4_frame::V4FrameArtifactMetadataV1;
use opencut_contracts::{AssetLockManifest, FrameRate, Time, text_frame_v2::FontLockV2};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const V4_CAPTION_VIDEO_MP4_KIND: &str = "render.project_caption_video.v4";

pub use super::project_v4_video::{
    SubmitV4VideoMp4V1 as SubmitV4CaptionVideoMp4V2, V4VideoExecutionSummaryV1,
};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct V4CaptionVideoMp4CapabilityV2 {
    pub schema_version: u32,
    pub available: bool,
    pub submit_tool: String,
    pub unavailable_reason: Option<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum V4CaptionVideoProfileV2 {
    #[serde(rename = "classic-v31-cfr-h264-aac-caption-v2")]
    ClassicV31CfrH264AacCaptionV2,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct V4CaptionVideoIdentityV2 {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub profile: V4CaptionVideoProfileV2,
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
pub struct V4CaptionVideoMp4ResultV2 {
    pub schema_version: u32,
    pub identity: V4CaptionVideoIdentityV2,
    pub asset_lock: AssetLockManifest,
    pub font_lock: Vec<FontLockV2>,
    pub render_input_sha256: String,
    pub execution: V4VideoExecutionSummaryV1,
    pub artifact: V4FrameArtifactMetadataV1,
}
