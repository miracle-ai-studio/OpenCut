//! Versioned provenance for a published project-render attempt.
use crate::reproducibility::ExecutionIdentityV1;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InspectExecutionV1 {
    pub job_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecutionReceiptV1 {
    pub schema_version: u32,
    pub job_id: String,
    pub attempt: u32,
    pub kind: String,
    pub request_sha256: String,
    pub source_revision: String,
    pub sequence_id: String,
    pub composition_sha256: String,
    pub execution: ExecutionIdentityV1,
    pub artifact_sha256: String,
    pub artifact_size: u64,
    pub covered_semantics: Vec<String>,
    /// Receipt records compilation coverage and output identity, not a perceptual
    /// oracle or a claim that another platform will produce identical bytes.
    pub semantic_output_verified: bool,
}
