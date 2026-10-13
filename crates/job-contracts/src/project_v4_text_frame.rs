//! Additive complete media/free-text frames; the media-only V1 stays frozen.
use opencut_contracts::{AssetLockManifest, Time, text_frame_v2::TextRasterReceiptV2};
pub use opencut_contracts::text_frame_v2::FontLockV2;
use super::project_v4_frame::{V4FrameExecutionSummaryV1,V4FrameSourceObservationV1,V4FrameArtifactMetadataV1};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
pub const V4_TEXT_FRAME_PNG_KIND: &str = "render.project_text_frame.v4";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubmitV4TextFramePng {
    pub idempotency_key: String,
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub time: Time,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct V4TextFramePngCapabilityV2 {
    pub schema_version: u32,
    pub available: bool,
    pub submit_tool: String,
    pub unavailable_reason: Option<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum V4TextFrameProfileV2 {
    #[serde(rename = "classic-v31-gpu-normal-text-frame-v2")]
    ClassicV31GpuNormalTextFrameV2,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct V4TextFrameIdentityV2 {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub time: Time,
    pub width: u32,
    pub height: u32,
    pub profile: V4TextFrameProfileV2,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct V4TextFramePngResultV2 {
    pub schema_version: u32,
    pub identity: V4TextFrameIdentityV2,
    pub asset_lock: AssetLockManifest,
    pub font_lock: Vec<FontLockV2>,
    pub render_input_sha256: String,
    pub execution: V4FrameExecutionSummaryV1,
    pub sources: Vec<V4FrameSourceObservationV1>,
    pub texts: Vec<TextRasterReceiptV2>,
    pub artifact: V4FrameArtifactMetadataV1,
}
