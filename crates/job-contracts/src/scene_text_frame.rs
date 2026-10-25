//! Additive complete media/free-text frames; the media-only V1 stays frozen.
use super::media_frame::{
    MediaFrameArtifactMetadata, MediaFrameExecutionSummary, MediaFrameSourceObservation,
};
pub use opencut_contracts::text_frame::FontLock;
use opencut_contracts::{AssetLockManifest, Time, text_frame::TextRasterReceipt};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
pub const TEXT_FRAME_PNG_KIND: &str = "render.project_text_frame.v4";
pub use crate::render_requests::FrameRenderInput;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TextFramePngCapability {
    pub schema_version: u32,
    pub available: bool,
    pub submit_tool: String,
    pub unavailable_reason: Option<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum TextFrameProfile {
    #[serde(rename = "classic-v31-gpu-normal-text-frame-v2")]
    ClassicV31GpuNormalTextFrameV2,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TextFrameJobIdentity {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub time: Time,
    pub width: u32,
    pub height: u32,
    pub profile: TextFrameProfile,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TextFramePngResult {
    pub schema_version: u32,
    pub identity: TextFrameJobIdentity,
    pub asset_lock: AssetLockManifest,
    pub font_lock: Vec<FontLock>,
    pub render_input_sha256: String,
    pub execution: MediaFrameExecutionSummary,
    pub sources: Vec<MediaFrameSourceObservation>,
    pub texts: Vec<TextRasterReceipt>,
    pub artifact: MediaFrameArtifactMetadata,
}
