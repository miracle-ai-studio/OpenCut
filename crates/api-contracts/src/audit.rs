//! Committed mutation provenance. These are outputs, never caller identity claims.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CallSource {
    Cli,
    Http,
    McpStdio,
    McpHttp,
    Embedded,
    Worker,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Actor {
    pub principal_id: String,
    pub source: CallSource,
    pub worker_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AuditRecord {
    pub sequence: String,
    pub audit_id: String,
    pub request_id: String,
    pub origin_request_id: Option<String>,
    pub actor: Actor,
    pub action: String,
    pub project_id: Option<String>,
    pub revision: Option<String>,
    pub transaction_id: Option<String>,
    pub job_id: Option<String>,
    pub attempt: Option<u32>,
    pub artifact_ids: Vec<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    content = "id",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum AuditTarget {
    Workspace,
    Project(String),
    Transaction(String),
    Job(String),
    Request(String),
}

fn first() -> String {
    "0".into()
}
fn page_size() -> usize {
    100
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AuditQuery {
    pub target: AuditTarget,
    #[serde(default = "first")]
    pub after_sequence: String,
    #[serde(default = "page_size")]
    pub limit: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AuditPage {
    pub records: Vec<AuditRecord>,
    pub last_sequence: String,
    pub has_more: bool,
    pub coverage: AuditCoverage,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AuditCoverage {
    RecordedMutationsSinceSchema4,
}
