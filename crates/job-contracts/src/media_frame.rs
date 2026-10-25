//! Additive, path-free fixed-revision Classic GPU scene frames.
use opencut_contracts::{AssetLockManifest, Time};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const MEDIA_FRAME_PNG_KIND: &str = "render.project_frame.v4";

pub use crate::render_requests::FrameRenderInput;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MediaFramePngCapability {
    pub schema_version: u32,
    /// Configured process availability; each job verifies its actual GPU/backend.
    pub available: bool,
    pub submit_tool: String,
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum MediaFrameProfile {
    #[serde(rename = "classic-v31-gpu-normal-frame")]
    ClassicV31GpuNormalFrame,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MediaFrameIdentity {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    /// Exact requested scene time; actual video observations are separate.
    pub time: Time,
    pub width: u32,
    pub height: u32,
    pub profile: MediaFrameProfile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MediaFrameNoSourceReason {
    OutsideReadWindow,
    Eof,
    TimestampGap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MediaFrameSourceObservation {
    pub element_id: String,
    pub asset_id: String,
    pub requested_time: Option<Time>,
    pub selected_video_stream: Option<u32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Mediabunny browser seconds, not a claim of exact original demux ticks.
    pub actual_timestamp_seconds: Option<f64>,
    pub frame_duration_seconds: Option<f64>,
    /// True only for an observed terminal video frame held by the authored
    /// layout. Omission retains the exact historical non-hold result shape.
    #[serde(default, skip_serializing_if = "is_false")]
    pub eof_held: bool,
    pub no_frame_reason: Option<MediaFrameNoSourceReason>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MediaFrameExecutionSummary {
    pub host_sha256: String,
    pub browser_version: String,
    pub browser_sha256: String,
    pub module_sha256: String,
    pub shader_sha256: String,
    pub playwright_version: String,
    pub mediabunny_version: String,
    pub gpu_backend: String,
    pub gpu_device: String,
    pub texture_format: String,
    pub decoder_profile: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MediaFrameArtifactMetadata {
    pub mime_type: String,
    pub size_bytes: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MediaFramePngResult {
    pub schema_version: u32,
    pub identity: MediaFrameIdentity,
    pub asset_lock: AssetLockManifest,
    pub render_input_sha256: String,
    pub execution: MediaFrameExecutionSummary,
    pub sources: Vec<MediaFrameSourceObservation>,
    pub artifact: MediaFrameArtifactMetadata,
}
