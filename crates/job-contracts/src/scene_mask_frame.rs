//! Additive parametric mask frames; previous media/text/caption contracts stay frozen.
use super::media_frame::{
    MediaFrameArtifactMetadata, MediaFrameExecutionSummary, MediaFrameSourceObservation,
};
pub use super::scene_text_frame::FrameRenderInput;
use opencut_contracts::{
    AssetLockManifest, Time,
    blend_frame::LayerBlend,
    caption_frame::CaptionLayoutReceipt,
    graphic_frame::GraphicRasterReceipt,
    mask_frame::LayerMask,
    text_frame::{FontLock, TextRasterReceipt},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
pub const MASK_FRAME_PNG_KIND: &str = "render.project_mask_frame.v4";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MaskFrameCapability {
    pub schema_version: u32,
    pub available: bool,
    pub submit_tool: String,
    pub unavailable_reason: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MaskFrameIdentity {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub time: Time,
    pub width: u32,
    pub height: u32,
    pub profile: MaskFrameProfile,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum MaskFrameProfile {
    #[serde(rename = "classic-v31-gpu-mask-frame-v1")]
    ClassicV31GpuMaskFrameV1,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MaskFramePngResult {
    pub schema_version: u32,
    pub identity: MaskFrameIdentity,
    pub asset_lock: AssetLockManifest,
    pub font_lock: Vec<FontLock>,
    pub render_input_sha256: String,
    pub execution: MediaFrameExecutionSummary,
    pub sources: Vec<MediaFrameSourceObservation>,
    pub texts: Vec<TextRasterReceipt>,
    pub captions: Vec<CaptionLayoutReceipt>,
    pub graphics: Vec<GraphicRasterReceipt>,
    pub blends: Vec<LayerBlend>,
    pub masks: Vec<LayerMask>,
    pub artifact: MediaFrameArtifactMetadata,
}
