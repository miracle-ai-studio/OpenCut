//! Owner-controlled project edit hold. The caller identity comes from the host.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditControlRead {
    pub project_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditControlSet {
    pub project_id: String,
    pub expected_generation: String,
    pub held: bool,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct EditControlView {
    pub project_id: String,
    pub generation: String,
    pub held: bool,
    pub held_by_principal: Option<String>,
}
