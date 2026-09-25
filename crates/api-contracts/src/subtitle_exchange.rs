//! UTF-8 subtitle interchange without host filesystem paths.
use opencut_contracts::{CaptionStyle, TransactionResult};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SubtitleFormat {
    Srt,
    Ass,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubtitleImport {
    pub project_id: String,
    pub expected_revision: String,
    pub sequence_id: String,
    pub idempotency_key: String,
    pub format: SubtitleFormat,
    pub content: String,
    /// SRT has no styles. For ASS this is a fallback when no style table exists.
    pub style: Option<CaptionStyle>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubtitleExport {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub format: SubtitleFormat,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubtitleImportReceipt {
    pub transaction: TransactionResult,
    pub cue_count: usize,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubtitleExportReceipt {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub format: SubtitleFormat,
    pub content: String,
    pub cue_count: usize,
    /// Interchange limitations, never hidden from the caller.
    pub losses: Vec<String>,
}
