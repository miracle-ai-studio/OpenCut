//! Explicit, reviewed approximation of the frozen 32-segment legacy lookup.
//! Planning never changes the document; callers apply the returned transaction.
use crate::project_v4_public::ApplyPublicV4Project;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeyframeLegacyConversionRequestV1 {
    pub project_id: String,
    pub expected_revision: String,
    pub idempotency_key: String,
    pub label: String,
    pub sequence_id: String,
    pub channel_id: String,
    /// Outgoing legacy segments, identified by their existing left key IDs.
    pub left_key_ids: Vec<String>,
    /// Absolute tolerance in the bound parameter's authored units, before any
    /// physical clamping. Below the conservative bound rejects the whole plan.
    pub maximum_absolute_error: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeyframeLegacyConversionResultV1 {
    pub plan: ApplyPublicV4Project,
    pub profile: String,
    pub converted_segments: u32,
    pub inserted_keys: u32,
    /// Largest difference at both one-sided boundaries of the original lookup
    /// intervals. This is a computed boundary statistic, not a zero-loss claim.
    pub max_absolute_error: f64,
    /// Boundary statistic plus a conservative floating arithmetic allowance;
    /// covers interior raw samples, decimal-12 jumps, and preserved endpoints.
    pub error_bound: f64,
}
