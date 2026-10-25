use opencut_contracts::export_video::ExportVideoOptions;
use opencut_job_contracts::render_requests::VideoFrameRange;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RenderPreflightInput {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub options: ExportVideoOptions,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<VideoFrameRange>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ProjectPreflightReport {
    pub revision: String,
    pub sequence_id: String,
    /// Ready to submit under current structural/resource/runtime checks.
    /// Execution still admits every sampled frame and its actual allocations;
    /// preflight does not render the entire output or guarantee future decoding.
    pub ready: bool,
    pub issues: Vec<PreflightIssue>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct PreflightIssue {
    pub code: String,
    pub message: String,
    pub asset_id: Option<String>,
    pub clip_id: Option<String>,
    pub cue_id: Option<String>,
}
