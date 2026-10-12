//! Additive, path-free fixed-revision Classic GPU scene frames.
use opencut_contracts::{AssetLockManifest, Time};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const V4_FRAME_PNG_KIND: &str = "render.project_frame.v4";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubmitV4FramePng {
    pub idempotency_key: String,
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub time: Time,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct V4FramePngCapabilityV1 {
    pub schema_version: u32,
    /// Configured process availability; each job verifies its actual GPU/backend.
    pub available: bool,
    pub submit_tool: String,
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum V4FrameProfileV1 {
    #[serde(rename = "classic-v31-gpu-normal-frame")]
    ClassicV31GpuNormalFrame,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct V4FrameIdentityV1 {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    /// Exact requested scene time; actual video observations are separate.
    pub time: Time,
    pub width: u32,
    pub height: u32,
    pub profile: V4FrameProfileV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum V4FrameNoSourceReasonV1 {
    OutsideReadWindow,
    Eof,
    TimestampGap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct V4FrameSourceObservationV1 {
    pub element_id: String,
    pub asset_id: String,
    pub requested_time: Option<Time>,
    pub selected_video_stream: Option<u32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Mediabunny browser seconds, not a claim of exact original demux ticks.
    pub actual_timestamp_seconds: Option<f64>,
    pub frame_duration_seconds: Option<f64>,
    pub no_frame_reason: Option<V4FrameNoSourceReasonV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct V4FrameExecutionSummaryV1 {
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
pub struct V4FrameArtifactMetadataV1 {
    pub mime_type: String,
    pub size_bytes: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct V4FramePngResultV1 {
    pub schema_version: u32,
    pub identity: V4FrameIdentityV1,
    pub asset_lock: AssetLockManifest,
    pub render_input_sha256: String,
    pub execution: V4FrameExecutionSummaryV1,
    pub sources: Vec<V4FrameSourceObservationV1>,
    pub artifact: V4FrameArtifactMetadataV1,
}
