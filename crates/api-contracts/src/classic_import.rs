//! Classic IndexedDB project JSON compatibility analysis, with no publication.
use opencut_contracts::ProjectDocument;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClassicAnalyze {
    /// SerializedProject JSON copied out of Classic's IndexedDB projects store.
    pub content: String,
    pub target_project_id: String,
    /// Classic media IDs are resolved from existing, fixed OpenCut project revisions by the host.
    pub media_mappings: Vec<ClassicMediaMapping>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClassicMediaMapping {
    pub classic_media_id: String,
    pub source_project_id: String,
    pub source_revision: String,
    pub asset_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompatibilityFinding {
    pub path: String,
    pub code: String,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClassicAnalysis {
    pub source_format: String,
    pub source_version: Option<u32>,
    pub source_content_id: String,
    pub target_project_id: String,
    pub mapped: Vec<String>,
    pub losses: Vec<CompatibilityFinding>,
    pub blockers: Vec<CompatibilityFinding>,
    /// Only set when every editing-relevant field has an explicit mapping.
    pub proposed_project: Option<ProjectDocument>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClassicImport {
    pub source: ClassicAnalyze,
    pub expected_source_content_id: String,
    pub idempotency_key: String,
    /// Explicitly acknowledge every reported loss code before publication.
    pub accepted_loss_codes: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClassicImportReceipt {
    pub target_project_id: String,
    pub revision: String,
    pub source_content_id: String,
    pub asset_count: usize,
    pub losses: Vec<CompatibilityFinding>,
}
