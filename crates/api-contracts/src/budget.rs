//! Durable admission policy. Logical payload limits are distinct from volume observations.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub revision: u64,
    pub payload_limit_bytes: String,
    pub minimum_available_bytes: String,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            revision: 0,
            payload_limit_bytes: (64u64 << 30).to_string(),
            minimum_available_bytes: (64u64 << 20).to_string(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Configure {
    pub expected_revision: u64,
    pub idempotency_key: String,
    pub payload_limit_bytes: String,
    pub minimum_available_bytes: String,
    /// Omission preserves the current ceiling, including for older consumers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_output_limit_bytes: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Writer {
    pub job_id: String,
    pub attempt: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PolicyChange {
    pub policy: Policy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_output_limit_bytes: Option<String>,
    pub request_id: String,
    pub actor: crate::audit::Actor,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Reservation {
    pub reservation_id: String,
    pub category: String,
    pub reserved_bytes: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub writer: Option<Writer>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Report {
    pub policy: Policy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_output_limit_bytes: Option<String>,
    pub last_policy_change: Option<PolicyChange>,
    pub observed_payload_bytes: String,
    pub reserved_bytes: String,
    pub admission_headroom_bytes: String,
    pub volume_available_bytes: String,
    pub reservations: Vec<Reservation>,
    pub recovered_reservations: usize,
    pub coverage: Vec<String>,
    pub enforces_all_workspace_writes: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Capabilities {
    pub inspect_tool: String,
    pub configure_tool: String,
    pub recover_tool: String,
    pub available: bool,
    pub owner_only: bool,
    pub max_active_reservations: usize,
    pub max_policy_changes: usize,
    pub enforces_all_workspace_writes: bool,
}
