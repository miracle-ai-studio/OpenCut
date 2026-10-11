use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RemoveMedia {
    pub project_id: String,
    pub expected_revision: String,
    pub asset_ids: Vec<String>,
    pub idempotency_key: String,
}
