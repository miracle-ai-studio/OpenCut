//! Fixed-revision configurable video export; previous MP4 result layouts frozen.
use super::{
    project_v4_audio::V4AudioExecutionSummaryV1,
    project_v4_frame::{V4FrameArtifactMetadataV1, V4FrameExecutionSummaryV1},
};
pub use opencut_contracts::export_video_v2::*;
use opencut_contracts::{AssetLockManifest, FrameRate, Time, text_frame_v2::FontLockV2};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
pub const EXPORT_VIDEO_V2_KIND: &str = "render.export_video.v2";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubmitExportVideoV2 {
    pub idempotency_key: String,
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub options: ExportVideoOptionsV2,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportVideoCapabilityV2 {
    pub schema_version: u32,
    pub available: bool,
    /// Audio-off export remains available without an audio renderer runtime.
    pub audio_available: bool,
    pub submit_tool: String,
    pub formats: Vec<ExportFormatV2>,
    pub qualities: Vec<ExportQualityV2>,
    pub unavailable_reason: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportVideoIdentityV2 {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub visual_profile: ExportVisualProfileV2,
    pub options: ExportVideoOptionsV2,
    pub width: u32,
    pub height: u32,
    pub fps: FrameRate,
    pub frame_count: String,
    pub timeline_duration: Time,
    pub video_duration: Time,
    /// None means there is no output audio stream, not a silent encoded stream.
    pub audio_frame_count: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportVideoEncoderSummaryV2 {
    pub ffmpeg_sha256: String,
    pub ffprobe_sha256: String,
    pub video_codec: String,
    pub audio_codec: Option<String>,
    pub pixel_format: String,
    pub rates: ExportVideoRatesV2,
    pub observed_video_frames: String,
    pub observed_video_duration_seconds: f64,
    pub observed_audio_duration_seconds: Option<f64>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportVideoExecutionV2 {
    pub visual: V4FrameExecutionSummaryV1,
    pub audio: Option<V4AudioExecutionSummaryV1>,
    pub encoder: ExportVideoEncoderSummaryV2,
    pub frame_receipts_sha256: String,
    pub frames_rendered: String,
    pub rgba_bytes: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportVideoResultV2 {
    pub schema_version: u32,
    pub identity: ExportVideoIdentityV2,
    pub asset_lock: AssetLockManifest,
    pub font_lock: Vec<FontLockV2>,
    pub render_input_sha256: String,
    pub execution: ExportVideoExecutionV2,
    pub artifact: V4FrameArtifactMetadataV1,
}
