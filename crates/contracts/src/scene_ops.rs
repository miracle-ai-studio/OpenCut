//! Versioned aggregate edits. Frozen V1/V2/V3 requests remain unchanged.
use crate::{AssetVersion, scene::*, scene_bookmarks::BookmarkOperation};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub use crate::project_catalog::CreateProjectRequest;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PromoteProject {
    pub project_id: String,
    /// Omit for main;an explicit value selects an existing named branch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_name: Option<String>,
    pub expected_revision: String,
    pub idempotency_key: String,
    /// Observed immutable identities; the host must verify their bytes and scope.
    #[serde(default)]
    pub asset_versions: BTreeMap<String, AssetVersion>,
}
pub use crate::project_inspection::InspectProject;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CopySceneProject {
    pub source_project_id: String,
    pub source_revision: String,
    pub project_id: String,
    pub name: String,
    pub idempotency_key: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApplySceneProject {
    pub project_id: String,
    pub expected_revision: String,
    pub idempotency_key: String,
    pub label: String,
    pub operations: Vec<ProjectOperation>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RestoreProject {
    pub project_id: String,
    /// Omit for main;an explicit value selects an existing named branch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_name: Option<String>,
    pub expected_revision: String,
    pub target_revision: String,
    pub idempotency_key: String,
    /// Optional original review label;omission preserves stored request identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Observed immutable identities for historical data lacking pinned media.
    /// Current editor resources cannot acquire overrides.
    #[serde(default)]
    pub asset_versions: BTreeMap<String, AssetVersion>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectOperation {
    Rename {
        name: String,
    },
    SettingsSet {
        settings: SettingsProfile,
    },
    DefaultSequenceSet {
        sequence_id: String,
    },
    AssetPut {
        asset: ProjectAsset,
    },
    AssetDelete {
        asset_id: String,
    },
    FontPut {
        font: FontResource,
    },
    FontDelete {
        font_id: String,
    },
    SequencePut {
        sequence: Scene,
    },
    SequenceDelete {
        sequence_id: String,
    },
    TrackPut {
        sequence_id: String,
        track: SceneTrack,
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
        element: Box<TimelineElement>,
    },
    ElementDelete {
        sequence_id: String,
        element_id: String,
    },
    CaptionPut {
        sequence_id: String,
        caption: Caption,
    },
    CaptionDelete {
        sequence_id: String,
        caption_id: String,
    },
    TransitionPut {
        sequence_id: String,
        transition: SceneTransition,
    },
    TransitionDelete {
        sequence_id: String,
        transition_id: String,
    },
    /// The shared bookmark request remains exact; its collection lives ONLY
    /// inside the immutable V4 sequence aggregate and is committed with it.
    Bookmark {
        operation: BookmarkOperation,
    },
}
