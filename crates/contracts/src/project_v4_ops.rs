//! Versioned aggregate edits. Frozen V1/V2/V3 requests remain unchanged.
use crate::{AssetVersion, CreateProject, project_v4::*, scene_bookmarks::BookmarkOperationV1};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateV4Project {
    pub project: CreateProject,
    pub idempotency_key: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PromoteV4Project {
    pub project_id: String,
    pub expected_revision: String,
    pub idempotency_key: String,
    /// Observed immutable identities; the host must verify their bytes and scope.
    pub asset_versions: BTreeMap<String, AssetVersion>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InspectV4Project {
    pub project_id: String,
    pub revision: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CopyV4Project {
    pub source_project_id: String,
    pub source_revision: String,
    pub project_id: String,
    pub name: String,
    pub idempotency_key: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListV4Projects {
    pub cursor: Option<String>,
    pub limit: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct V4ProjectSummary {
    pub project_id: String,
    pub name: String,
    pub revision: String,
    /// Protocol V4 library summaries may identify legacy heads without casting
    /// their complete document into a frozen legacy snapshot DTO.
    pub schema_version: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct V4ProjectPage {
    pub items: Vec<V4ProjectSummary>,
    pub next_cursor: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApplyV4Project {
    pub project_id: String,
    pub expected_revision: String,
    pub idempotency_key: String,
    pub label: String,
    pub operations: Vec<ProjectOperationV4>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RestoreV4Revision {
    pub project_id: String,
    pub expected_revision: String,
    pub target_revision: String,
    pub idempotency_key: String,
    /// Used only when restoring V1/V2/V3 history; V4 cannot acquire overrides.
    pub asset_versions: BTreeMap<String, AssetVersion>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectOperationV4 {
    Rename {
        name: String,
    },
    SettingsSet {
        settings: SettingsProfileV4,
    },
    DefaultSequenceSet {
        sequence_id: String,
    },
    AssetPut {
        asset: AssetV4,
    },
    AssetDelete {
        asset_id: String,
    },
    FontPut {
        font: FontResourceV4,
    },
    FontDelete {
        font_id: String,
    },
    SequencePut {
        sequence: SequenceV4,
    },
    SequenceDelete {
        sequence_id: String,
    },
    TrackPut {
        sequence_id: String,
        track: TrackV4,
    },
    TrackDelete {
        sequence_id: String,
        track_id: String,
    },
    /// Only the explicit lock operation may unlock a protected track; replacing
    /// an entire track/sequence cannot bypass its current lock.
    TrackLockSet {
        sequence_id: String,
        track_id: String,
        locked: bool,
    },
    ElementPut {
        sequence_id: String,
        track_id: String,
        element: Box<TimelineElementV4>,
    },
    ElementDelete {
        sequence_id: String,
        element_id: String,
    },
    CaptionPut {
        sequence_id: String,
        caption: CaptionV4,
    },
    CaptionDelete {
        sequence_id: String,
        caption_id: String,
    },
    TransitionPut {
        sequence_id: String,
        transition: TransitionV4,
    },
    TransitionDelete {
        sequence_id: String,
        transition_id: String,
    },
    /// The shared bookmark request remains exact; its collection lives ONLY
    /// inside the immutable V4 sequence aggregate and is committed with it.
    Bookmark {
        operation: BookmarkOperationV1,
    },
}
