//! Additive pinned-caption frames; previous media/text frame contracts stay frozen.
use super::project_v4_frame::{
    V4FrameArtifactMetadataV1, V4FrameExecutionSummaryV1, V4FrameSourceObservationV1,
};
pub use super::project_v4_text_frame::SubmitV4TextFramePng as SubmitV4CaptionFramePng;
use opencut_contracts::{
    AssetLockManifest, Time,
    text_frame_v2::{FontLockV2, TextRasterReceiptV2},
    text_frame_v3::CaptionLayoutReceiptV3,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
pub const V4_CAPTION_FRAME_PNG_KIND: &str = "render.project_caption_frame.v4";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct V4CaptionFrameCapabilityV3 {
    pub schema_version: u32,
    pub available: bool,
    pub submit_tool: String,
    pub unavailable_reason: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct V4CaptionFrameIdentityV3 {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub time: Time,
    pub width: u32,
    pub height: u32,
    pub profile: V4CaptionFrameProfileV3,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum V4CaptionFrameProfileV3 {
    #[serde(rename = "classic-v31-gpu-normal-caption-frame-v3")]
    ClassicV31GpuNormalCaptionFrameV3,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct V4CaptionFramePngResultV3 {
    pub schema_version: u32,
    pub identity: V4CaptionFrameIdentityV3,
    pub asset_lock: AssetLockManifest,
    pub font_lock: Vec<FontLockV2>,
    pub render_input_sha256: String,
    pub execution: V4FrameExecutionSummaryV1,
    pub sources: Vec<V4FrameSourceObservationV1>,
    pub texts: Vec<TextRasterReceiptV2>,
    pub captions: Vec<CaptionLayoutReceiptV3>,
    pub artifact: V4FrameArtifactMetadataV1,
}
