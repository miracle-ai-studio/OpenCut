//! Additive parametric scene frames; previous media/text/caption contracts stay frozen.
use super::media_frame::{
    MediaFrameArtifactMetadata, MediaFrameExecutionSummary, MediaFrameSourceObservation,
};
pub use super::scene_text_frame::FrameRenderInput;
use opencut_contracts::{
    AssetLockManifest, Time,
    blend_frame::LayerBlend,
    caption_frame::CaptionLayoutReceipt,
    graphic_frame::GraphicRasterReceipt,
    scene_frame::{SceneBackground, SceneLayerStack, SceneStep},
    text_frame::{FontLock, TextRasterReceipt},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
pub const SCENE_FRAME_PNG_KIND: &str = "render.project_scene_frame.v4";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SceneFrameCapability {
    pub schema_version: u32,
    pub available: bool,
    pub submit_tool: String,
    pub unavailable_reason: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SceneFrameIdentity {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub time: Time,
    pub width: u32,
    pub height: u32,
    pub profile: SceneFrameProfile,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum SceneFrameProfile {
    #[serde(rename = "classic-v31-gpu-scene-frame-v1")]
    ClassicV31GpuSceneFrameV1,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SceneFramePngResult {
    pub schema_version: u32,
    pub identity: SceneFrameIdentity,
    pub asset_lock: AssetLockManifest,
    pub font_lock: Vec<FontLock>,
    pub render_input_sha256: String,
    pub execution: MediaFrameExecutionSummary,
    pub sources: Vec<MediaFrameSourceObservation>,
    pub texts: Vec<TextRasterReceipt>,
    pub captions: Vec<CaptionLayoutReceipt>,
    pub graphics: Vec<GraphicRasterReceipt>,
    pub blends: Vec<LayerBlend>,
    pub stacks: Vec<SceneLayerStack>,
    pub steps: Vec<SceneStep>,
    pub background: SceneBackground,
    pub artifact: MediaFrameArtifactMetadata,
}
