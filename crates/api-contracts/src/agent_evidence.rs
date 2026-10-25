//! Fixed-version content evidence and explicit source/sequence clock mapping.
use crate::analysis_evidence::AnalysisEvidencePage;
use opencut_contracts::{
    AssetVersion, Time, TimeRange, scene::RetimeMapping, timeline::TimelineTarget,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContentSearch {
    pub project_id: String,
    pub revision: String,
    /// Explicit successful analysis identities, 1..32. Existing results are reused.
    pub analysis_job_ids: Vec<String>,
    pub text: Option<String>,
    pub source_range: Option<TimeRange>,
    /// Per-analysis pagination remains bound by its original producer/caller.
    pub cursors: std::collections::BTreeMap<String, String>,
    pub limit_per_analysis: u16,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContentSearchResult {
    pub project_id: String,
    pub revision: String,
    /// Provenance and original analysis revision are retained. Every asset
    /// version is checked against the requested V4 revision before returning it.
    pub pages: Vec<AnalysisEvidencePage>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MapDirection {
    SourceToSequence,
    SequenceToSource,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MapMediaTime {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub target: TimelineTarget,
    pub direction: MapDirection,
    pub times: Vec<Time>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MappedMediaPoint {
    pub input: Time,
    pub mapped: Time,
    /// Quantized inverse mappings are not claimed to be exact inverses.
    pub roundtrip: Time,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MappedMediaTimes {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub target: TimelineTarget,
    pub asset_id: String,
    pub asset_version: AssetVersion,
    pub mapping: RetimeMapping,
    pub source_range: TimeRange,
    pub sequence_range: TimeRange,
    pub points: Vec<MappedMediaPoint>,
}
