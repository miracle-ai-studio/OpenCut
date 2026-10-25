//! Project library metadata and atomic owner-managed duplication.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use opencut_contracts::project_catalog::{
    LibraryInput, LibraryPage, LibraryProject, ProjectFormat,
};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CopyProject {
    pub source_project_id: String,
    pub expected_revision: String,
    pub target_project_id: String,
    pub name: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DuplicateProjects {
    pub projects: Vec<CopyProject>,
    pub idempotency_key: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProjectCopy {
    pub source_project_id: String,
    pub source_revision: String,
    pub project_id: String,
    pub revision: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DuplicateReceipt {
    pub projects: Vec<ProjectCopy>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeleteProject {
    pub project_id: String,
    pub expected_revision: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeleteProjects {
    pub projects: Vec<DeleteProject>,
    pub idempotency_key: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DeleteReceipt {
    pub projects: Vec<DeleteProject>,
    pub staging_upload_ids: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DeleteOutcome {
    pub receipt: DeleteReceipt,
    pub cleanup_pending: Vec<String>,
}
