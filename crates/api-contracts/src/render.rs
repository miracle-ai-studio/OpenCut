use opencut_media_contracts::{Container, VideoCodec};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RenderPreflightInput {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub container: Container,
    pub video_codec: VideoCodec,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ProjectPreflightReport {
    pub revision: String,
    pub sequence_id: String,
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
