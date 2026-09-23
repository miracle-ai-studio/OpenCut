//! Owner-only bounded authorization denial history, separate from successful mutations.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DeniedOperation {
    Tool { name: String },
    UnknownTool,
    Authentication,
    HttpRequest,
    CliRequest,
    McpRequest,
    UploadChunk,
    Stream,
    Administration,
    HostStart,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DenialReason {
    AuthenticationRejected,
    OriginRejected,
    AuthorizationDenied,
    PolicyChanged,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DenialRecord {
    pub sequence: String,
    pub request_id: String,
    pub source: crate::audit::CallSource,
    pub principal_id: Option<String>,
    pub credential_id: Option<String>,
    pub operation: DeniedOperation,
    pub reason: DenialReason,
    pub created_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    content = "id",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum DenialTarget {
    Workspace,
    Request(String),
    Principal(String),
    Credential(String),
}
fn first() -> String {
    "0".into()
}
fn limit() -> usize {
    100
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DenialQuery {
    pub target: DenialTarget,
    #[serde(default = "first")]
    pub after_sequence: String,
    #[serde(default = "limit")]
    pub limit: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DenialPage {
    pub records: Vec<DenialRecord>,
    pub last_sequence: String,
    pub has_more: bool,
    pub oldest_available_sequence: Option<String>,
    pub max_retained_records: usize,
    pub coverage: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DenialCapabilities {
    pub query_tool: String,
    pub max_retained_records: usize,
    pub failure_warning: String,
    pub request_id_header: String,
    pub captures_request_bodies: bool,
}
