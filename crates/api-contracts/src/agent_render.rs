//! One render contract; output and range are user choices, never protocol choices.
use opencut_contracts::export_video::{ExportFormat, ExportQuality, ExportVideoOptions};
pub use opencut_job_contracts::render_requests::{
    AudioSampleRange, FrameRenderInput, VideoFrameRange,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioRenderInput {
    pub idempotency_key: String,
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub sample_rate: u32,
    /// None renders the whole scene through the same audio pipeline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<AudioSampleRange>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VideoRenderInput {
    pub idempotency_key: String,
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub options: ExportVideoOptions,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<VideoFrameRange>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    content = "request",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum RenderRequest {
    Frame(FrameRenderInput),
    Audio(AudioRenderInput),
    Video(VideoRenderInput),
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RenderCapability {
    pub frame: RenderAvailability,
    pub audio: RenderAvailability,
    pub video: RenderAvailability,
    pub video_audio: bool,
    pub formats: Vec<ExportFormat>,
    pub qualities: Vec<ExportQuality>,
    pub sample_rates: Vec<u32>,
    pub submit_tool: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RenderAvailability {
    pub available: bool,
    pub unavailable_reason: Option<String>,
}
