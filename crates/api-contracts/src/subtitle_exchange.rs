//! UTF-8 subtitle interchange without host filesystem paths.
pub use opencut_contracts::subtitles::{CaptionReceipt, SubtitleExport, SubtitleFormat};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
