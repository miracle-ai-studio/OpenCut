//! Portable project directory and creation declarations.
use crate::{FrameRate, Time};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LibraryInput {
    pub cursor: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProjectFormat {
    Base,
    Visual,
    Editor,
    Readonly,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct LibraryProject {
    pub project_id: String,
    pub name: String,
    pub revision: String,
    pub format: ProjectFormat,
    pub created_at: String,
    pub modified_at: String,
    pub duration: Time,
    pub width: u32,
    pub height: u32,
    pub fps: FrameRate,
    pub scene_count: usize,
    pub asset_count: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct LibraryPage {
    pub items: Vec<LibraryProject>,
    pub next_cursor: Option<String>,
}

fn default_limit() -> usize {
    100
}
/// The only public creation request;old stored receipts use private readers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateProjectRequest {
    pub project: crate::CreateProject,
    pub idempotency_key: String,
}
/// An immutable creation receipt. Open the project through the directory/read
/// APIs;the current head may have moved since this original creation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectCreated {
    pub project_id: String,
    pub revision: String,
}
