//! Technical quality and sampled caption geometry are separate claims.
use crate::job_views::ArtifactReference;
use opencut_contracts::Time;
pub use opencut_media_contracts::quality::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QualityInspect {
    pub artifact_id: String,
    /// Optional existing scene-frame jobs; no implicit render or new fixture.
    pub caption_frame_job_ids: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CaptionBoundsSample {
    pub job_id: String,
    pub time: Time,
    pub caption_id: String,
    pub outside_canvas: bool,
    pub same_runtime_as_export: bool,
    pub visual: opencut_job_contracts::media_frame::MediaFrameExecutionSummary,
    /// Conservative transformed measured layout rectangles, not alpha-pixel proof.
    pub bounds: [f64; 4],
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QualityReport {
    pub artifact: ArtifactReference,
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub ffmpeg_sha256: String,
    pub ffprobe_sha256: String,
    pub measurements: OutputMeasurements,
    pub technical_findings: Vec<String>,
    pub timestamp_av_offset_seconds: Option<f64>,
    pub caption_samples: Vec<CaptionBoundsSample>,
    pub temporal_caption_findings: Vec<String>,
    pub caption_coverage: String,
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QualityEvidenceRequest {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    /// Read-only planning identity; child render keys bind this whole request.
    pub request_key: String,
    pub max_frames: u16,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QualityEvidenceFrame {
    pub reasons: Vec<String>,
    pub request: opencut_job_contracts::scene_frame::FrameRenderInput,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QualityEvidencePlan {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub frames: Vec<QualityEvidenceFrame>,
    pub candidate_frames: usize,
    pub omitted_frames: usize,
    pub coverage: String,
}
