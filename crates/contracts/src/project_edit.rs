//! Project and scene structure commands, separate from frozen visual-edit operations.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectEditRequest {
    pub project_id: String,
    pub expected_revision: String,
    pub idempotency_key: String,
    pub label: String,
    pub operations: Vec<ProjectEdit>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectEdit {
    RenameProject {
        name: String,
    },
    CreateScene {
        sequence_id: String,
        name: String,
        video_track_id: String,
    },
    RenameScene {
        sequence_id: String,
        name: String,
    },
    DeleteScene {
        sequence_id: String,
    },
}
