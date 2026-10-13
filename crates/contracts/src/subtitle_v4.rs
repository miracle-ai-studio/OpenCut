//! V4 caption construction and interchange; pinned fonts and owning tracks are explicit.

use crate::{TransactionResult, project_v4::CaptionStyleV4};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SubtitleFormatV4 {
    Srt,
    Ass,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GenerateCaptionsV4 {
    pub project_id: String,
    pub expected_revision: String,
    pub sequence_id: String,
    pub track_id: String,
    pub analysis_job_id: String,
    pub idempotency_key: String,
    pub words_per_caption: u8,
    pub style: CaptionStyleV4,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubtitleImportV4 {
    pub project_id: String,
    pub expected_revision: String,
    pub sequence_id: String,
    pub track_id: String,
    pub idempotency_key: String,
    pub format: SubtitleFormatV4,
    pub content: String,
    pub style: CaptionStyleV4,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubtitleExportV4 {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub track_id: Option<String>,
    pub format: SubtitleFormatV4,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CaptionReceiptV4 {
    pub transaction: TransactionResult,
    pub cue_count: usize,
    pub warnings: Vec<String>,
}
