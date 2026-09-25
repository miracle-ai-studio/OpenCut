//! Bounded, source-linked evidence queries over immutable successful analyses.
use opencut_contracts::{LockedAsset, TimeRange};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

fn default_limit() -> usize {
    20
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AnalysisQueryInput {
    pub job_id: String,
    /// Case-insensitive literal substring, not a regular expression.
    #[serde(default)]
    pub text: Option<String>,
    /// Source-media range, independent of any timeline placement.
    #[serde(default)]
    pub source_range: Option<TimeRange>,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AnalysisEvidence {
    pub evidence_id: String,
    pub segment_index: u32,
    pub source_range: TimeRange,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AnalysisPage {
    pub analysis_job_id: String,
    pub project_id: String,
    pub revision: String,
    pub asset_id: String,
    pub asset_version: LockedAsset,
    pub provider: String,
    pub model: String,
    /// Exact model bytes when recorded by the producer; absent for older results.
    pub model_sha256: Option<String>,
    pub evidence: Vec<AnalysisEvidence>,
    pub next_cursor: Option<String>,
}
