//! Revision-owned editor annotations. This model is not part of frozen V2/V3 DTOs.
//! Host persistence must commit annotations together with the complete project;
//! this module does not introduce a separately mutable bookmark store.
use crate::Time;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SceneBookmarkV1 {
    pub id: String,
    pub sequence_id: String,
    pub time: Time,
    pub duration: Option<Time>,
    pub color: Option<String>,
    pub note: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BookmarkOperationV1 {
    Toggle {
        sequence_id: String,
        time: Time,
        /// Stable client-supplied identity, used only when inserting.
        bookmark_id: String,
    },
    Put {
        bookmark: SceneBookmarkV1,
    },
    Move {
        sequence_id: String,
        bookmark_id: String,
        time: Time,
    },
    Delete {
        sequence_id: String,
        bookmark_id: String,
    },
}
