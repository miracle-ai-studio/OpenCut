//! Additive pinned-caption frames; previous media/text frame contracts stay frozen.
use super::media_frame::{
    MediaFrameArtifactMetadata, MediaFrameExecutionSummary, MediaFrameSourceObservation,
};
pub use super::scene_text_frame::FrameRenderInput;
use opencut_contracts::{
    AssetLockManifest, Time,
    caption_frame::CaptionLayoutReceipt,
    text_frame::{FontLock, TextRasterReceipt},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
pub const CAPTION_FRAME_PNG_KIND: &str = "render.project_caption_frame.v4";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CaptionFrameCapability {
    pub schema_version: u32,
    pub available: bool,
    pub submit_tool: String,
    pub unavailable_reason: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CaptionFrameIdentity {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub time: Time,
    pub width: u32,
    pub height: u32,
    pub profile: CaptionFrameProfile,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum CaptionFrameProfile {
    #[serde(rename = "classic-v31-gpu-normal-caption-frame-v3")]
    ClassicV31GpuNormalCaptionFrameV3,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CaptionFramePngResult {
    pub schema_version: u32,
    pub identity: CaptionFrameIdentity,
    pub asset_lock: AssetLockManifest,
    pub font_lock: Vec<FontLock>,
    pub render_input_sha256: String,
    pub execution: MediaFrameExecutionSummary,
    pub sources: Vec<MediaFrameSourceObservation>,
    pub texts: Vec<TextRasterReceipt>,
    pub captions: Vec<CaptionLayoutReceipt>,
    pub artifact: MediaFrameArtifactMetadata,
}
