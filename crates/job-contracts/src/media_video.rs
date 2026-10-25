//! Additive fixed-scene CFR output. Rendering stays on the V4 shared backend.
use super::media_frame::{MediaFrameArtifactMetadata, MediaFrameExecutionSummary};
use super::scene_audio::AudioExecutionSummary;
use opencut_contracts::{AssetLockManifest, FrameRate, Time, text_frame::FontLock};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const MEDIA_VIDEO_MP4_KIND: &str = "render.project_video.v4";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FixedVideoInput {
    pub idempotency_key: String,
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MediaVideoMp4Capability {
    pub schema_version: u32,
    pub available: bool,
    pub submit_tool: String,
    pub unavailable_reason: Option<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum MediaVideoProfile {
    #[serde(rename = "classic-v31-cfr-h264-aac-v1")]
    ClassicV31CfrH264AacV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MediaVideoIdentity {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub profile: MediaVideoProfile,
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
pub struct MediaVideoEncoderSummary {
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
pub struct MediaVideoExecutionSummary {
    pub visual: MediaFrameExecutionSummary,
    pub audio: AudioExecutionSummary,
    pub encoder: MediaVideoEncoderSummary,
    /// Ordered digest of individually validated frame receipts, not all frames
    /// retained in a public response or a claim of exact codec reproducibility.
    pub frame_receipts_sha256: String,
    pub frames_rendered: String,
    pub rgba_bytes: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MediaVideoMp4Result {
    pub schema_version: u32,
    pub identity: MediaVideoIdentity,
    pub asset_lock: AssetLockManifest,
    pub font_lock: Vec<FontLock>,
    pub render_input_sha256: String,
    pub execution: MediaVideoExecutionSummary,
    pub artifact: MediaFrameArtifactMetadata,
}
