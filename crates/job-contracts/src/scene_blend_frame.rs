//! Additive parametric blend frames; previous media/text/caption contracts stay frozen.
use super::media_frame::{
    MediaFrameArtifactMetadata, MediaFrameExecutionSummary, MediaFrameSourceObservation,
};
pub use super::scene_text_frame::FrameRenderInput;
use opencut_contracts::{
    AssetLockManifest, Time,
    blend_frame::LayerBlend,
    caption_frame::CaptionLayoutReceipt,
    graphic_frame::GraphicRasterReceipt,
    text_frame::{FontLock, TextRasterReceipt},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
pub const BLEND_FRAME_PNG_KIND: &str = "render.project_blend_frame.v4";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BlendFrameCapability {
    pub schema_version: u32,
    pub available: bool,
    pub submit_tool: String,
    pub unavailable_reason: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BlendFrameIdentity {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub time: Time,
    pub width: u32,
    pub height: u32,
    pub profile: BlendFrameProfile,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum BlendFrameProfile {
    #[serde(rename = "classic-v31-gpu-blend-frame-v1")]
    ClassicV31GpuBlendFrameV1,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BlendFramePngResult {
    pub schema_version: u32,
    pub identity: BlendFrameIdentity,
    pub asset_lock: AssetLockManifest,
    pub font_lock: Vec<FontLock>,
    pub render_input_sha256: String,
    pub execution: MediaFrameExecutionSummary,
    pub sources: Vec<MediaFrameSourceObservation>,
    pub texts: Vec<TextRasterReceipt>,
    pub captions: Vec<CaptionLayoutReceipt>,
    pub graphics: Vec<GraphicRasterReceipt>,
    pub blends: Vec<LayerBlend>,
    pub artifact: MediaFrameArtifactMetadata,
}
