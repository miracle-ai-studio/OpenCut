//! Caption construction and interchange; pinned fonts and owning tracks are explicit.

use crate::{TransactionResult, scene::SceneCaptionStyle};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SubtitleFormat {
    Srt,
    Ass,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GenerateCaptions {
    pub project_id: String,
    pub expected_revision: String,
    pub sequence_id: String,
    pub track_id: String,
    pub analysis_job_id: String,
    pub idempotency_key: String,
    pub words_per_caption: u8,
    pub style: SceneCaptionStyle,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubtitleImport {
    pub project_id: String,
    pub expected_revision: String,
    pub sequence_id: String,
    pub track_id: String,
    pub idempotency_key: String,
    pub format: SubtitleFormat,
    pub content: String,
    pub style: SceneCaptionStyle,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubtitleExport {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub track_id: Option<String>,
    pub format: SubtitleFormat,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CaptionReceipt {
    pub transaction: TransactionResult,
    pub cue_count: usize,
    pub warnings: Vec<String>,
}
