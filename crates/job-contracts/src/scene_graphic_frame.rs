//! Additive parametric graphic frames; previous media/text/caption contracts stay frozen.
use super::media_frame::{
    MediaFrameArtifactMetadata, MediaFrameExecutionSummary, MediaFrameSourceObservation,
};
pub use super::scene_text_frame::FrameRenderInput;
use opencut_contracts::{
    AssetLockManifest, Time,
    caption_frame::CaptionLayoutReceipt,
    graphic_frame::GraphicRasterReceipt,
    text_frame::{FontLock, TextRasterReceipt},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
pub const GRAPHIC_FRAME_PNG_KIND: &str = "render.project_graphic_frame.v4";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphicFrameCapability {
    pub schema_version: u32,
    pub available: bool,
    pub submit_tool: String,
    pub unavailable_reason: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphicFrameIdentity {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub time: Time,
    pub width: u32,
    pub height: u32,
    pub profile: GraphicFrameProfile,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum GraphicFrameProfile {
    #[serde(rename = "classic-v31-gpu-normal-graphic-frame-v1")]
    ClassicV31GpuNormalGraphicFrameV1,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphicFramePngResult {
    pub schema_version: u32,
    pub identity: GraphicFrameIdentity,
    pub asset_lock: AssetLockManifest,
    pub font_lock: Vec<FontLock>,
    pub render_input_sha256: String,
    pub execution: MediaFrameExecutionSummary,
    pub sources: Vec<MediaFrameSourceObservation>,
    pub texts: Vec<TextRasterReceipt>,
    pub captions: Vec<CaptionLayoutReceipt>,
    pub graphics: Vec<GraphicRasterReceipt>,
    pub artifact: MediaFrameArtifactMetadata,
}
