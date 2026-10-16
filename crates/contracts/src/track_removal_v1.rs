//! Additive read-only whole-track removal planning. Existing empty-track commands
//! keep their original admission; the result is a frozen public V4 transaction.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlanTrackRemovalV1 {
    pub project_id: String,
    pub expected_revision: String,
    pub idempotency_key: String,
    pub label: String,
    pub sequence_id: String,
    pub track_id: String,
}
