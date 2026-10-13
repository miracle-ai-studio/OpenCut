//! Additive fixed-scene CFR output. Rendering stays on the V4 shared backend.
use super::project_v4_audio::V4AudioExecutionSummaryV1;
use super::project_v4_frame::{V4FrameArtifactMetadataV1, V4FrameExecutionSummaryV1};
use opencut_contracts::{AssetLockManifest, FrameRate, Time, text_frame_v2::FontLockV2};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const V4_VIDEO_MP4_KIND: &str = "render.project_video.v4";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubmitV4VideoMp4V1 {
    pub idempotency_key: String,
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct V4VideoMp4CapabilityV1 {
    pub schema_version: u32,
    pub available: bool,
    pub submit_tool: String,
    pub unavailable_reason: Option<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum V4VideoProfileV1 {
    #[serde(rename = "classic-v31-cfr-h264-aac-v1")]
    ClassicV31CfrH264AacV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct V4VideoIdentityV1 {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub profile: V4VideoProfileV1,
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
pub struct V4VideoEncoderSummaryV1 {
    pub ffmpeg_sha256: String,
    pub ffprobe_sha256: String,
    pub video_codec: String,
    pub audio_codec: String,
    pub pixel_format: String,
    pub crf: u32,
    pub audio_bitrate: u32,
    pub observed_video_frames: String,
    pub observed_video_duration_seconds: f64,
    pub observed_audio_duration_seconds: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct V4VideoExecutionSummaryV1 {
    pub visual: V4FrameExecutionSummaryV1,
    pub audio: V4AudioExecutionSummaryV1,
    pub encoder: V4VideoEncoderSummaryV1,
    /// Ordered digest of individually validated frame receipts, not all frames
    /// retained in a public response or a claim of exact codec reproducibility.
    pub frame_receipts_sha256: String,
    pub frames_rendered: String,
    pub rgba_bytes: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct V4VideoMp4ResultV1 {
    pub schema_version: u32,
    pub identity: V4VideoIdentityV1,
    pub asset_lock: AssetLockManifest,
    pub font_lock: Vec<FontLockV2>,
    pub render_input_sha256: String,
    pub execution: V4VideoExecutionSummaryV1,
    pub artifact: V4FrameArtifactMetadataV1,
}
