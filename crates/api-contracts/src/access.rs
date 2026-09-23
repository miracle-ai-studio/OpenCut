use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProjectAction {
    Read,
    Edit,
    Restore,
    RunJobs,
    CancelJobs,
    ReadArtifacts,
    ReadAudit,
}
impl ProjectAction {
    pub fn mask(self) -> u32 {
        1 << match self {
            Self::Read => 0,
            Self::Edit => 1,
            Self::Restore => 2,
            Self::RunJobs => 3,
            Self::CancelJobs => 4,
            Self::ReadArtifacts => 5,
            Self::ReadAudit => 6,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectGrant {
    pub project_id: String,
    pub actions: Vec<ProjectAction>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PrincipalPolicy {
    pub principal_id: String,
    pub expected_revision: String,
    pub enabled: bool,
    pub grants: Vec<ProjectGrant>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PrincipalView {
    pub principal_id: String,
    pub revision: String,
    pub enabled: bool,
    pub grants: Vec<ProjectGrant>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct IssuedCredential {
    pub principal_id: String,
    pub credential_id: String,
    pub secret: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AccessView {
    pub principal_id: String,
    pub credential_id: Option<String>,
    pub owner: bool,
    pub policy_revision: Option<String>,
    pub grants: Vec<ProjectGrant>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum AccessAuditAction {
    #[serde(rename = "policy.updated")]
    PolicyUpdated,
    #[serde(rename = "credential.issued")]
    CredentialIssued,
    #[serde(rename = "credential.revoked")]
    CredentialRevoked,
}

/// Administration history, never a credential or an authorization decision.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AccessAuditRecord {
    pub sequence: String,
    pub principal_id: String,
    pub credential_id: Option<String>,
    pub action: AccessAuditAction,
    pub request_id: String,
    pub actor: crate::audit::Actor,
    /// Policy as committed at this event; later changes do not rewrite it.
    pub policy: Option<PrincipalView>,
    pub created_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    content = "id",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum AccessAuditTarget {
    Workspace,
    Principal(String),
    Credential(String),
    Request(String),
}
fn first_sequence() -> String {
    "0".into()
}
fn audit_page_size() -> usize {
    100
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AccessAuditQuery {
    pub target: AccessAuditTarget,
    #[serde(default = "first_sequence")]
    pub after_sequence: String,
    #[serde(default = "audit_page_size")]
    pub limit: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AccessAuditCoverage {
    RecordedAdministrationSinceSchema5,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AccessAuditPage {
    pub records: Vec<AccessAuditRecord>,
    pub last_sequence: String,
    pub has_more: bool,
    pub coverage: AccessAuditCoverage,
}
