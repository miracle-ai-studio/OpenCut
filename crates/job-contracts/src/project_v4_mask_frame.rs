//! Additive parametric mask frames; previous media/text/caption contracts stay frozen.
use super::project_v4_frame::{
    V4FrameArtifactMetadataV1, V4FrameExecutionSummaryV1, V4FrameSourceObservationV1,
};
pub use super::project_v4_text_frame::SubmitV4TextFramePng as SubmitV4MaskFramePng;
use opencut_contracts::{
    AssetLockManifest, Time,
    blend_frame_v1::LayerBlendV1,
    graphic_frame_v1::GraphicRasterReceiptV1,
    mask_frame_v1::LayerMaskV1,
    text_frame_v2::{FontLockV2, TextRasterReceiptV2},
    text_frame_v3::CaptionLayoutReceiptV3,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
pub const V4_MASK_FRAME_PNG_KIND: &str = "render.project_mask_frame.v4";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct V4MaskFrameCapabilityV1 {
    pub schema_version: u32,
    pub available: bool,
    pub submit_tool: String,
    pub unavailable_reason: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct V4MaskFrameIdentityV1 {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub time: Time,
    pub width: u32,
    pub height: u32,
    pub profile: V4MaskFrameProfileV1,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum V4MaskFrameProfileV1 {
    #[serde(rename = "classic-v31-gpu-mask-frame-v1")]
    ClassicV31GpuMaskFrameV1,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct V4MaskFramePngResultV1 {
    pub schema_version: u32,
    pub identity: V4MaskFrameIdentityV1,
    pub asset_lock: AssetLockManifest,
    pub font_lock: Vec<FontLockV2>,
    pub render_input_sha256: String,
    pub execution: V4FrameExecutionSummaryV1,
    pub sources: Vec<V4FrameSourceObservationV1>,
    pub texts: Vec<TextRasterReceiptV2>,
    pub captions: Vec<CaptionLayoutReceiptV3>,
    pub graphics: Vec<GraphicRasterReceiptV1>,
    pub blends: Vec<LayerBlendV1>,
    pub masks: Vec<LayerMaskV1>,
    pub artifact: V4FrameArtifactMetadataV1,
}
