use crate::{AssetVersion, scene::*, scene_bookmarks::BookmarkOperation};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AssetSelector {
    /// Project is implicit in the enclosing authorized edit request.
    ProjectAsset { revision: String, asset_id: String },
    /// A scoped media observation, not an arbitrary CAS-hash capability.
    ProjectMedia {
        media_id: String,
        version: AssetVersion,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FontSelector {
    ProjectFont { revision: String, font_id: String },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApplyProject {
    pub project_id: String,
    /// Omit for main;an explicit value selects an existing named branch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_name: Option<String>,
    pub expected_revision: String,
    pub idempotency_key: String,
    pub label: String,
    pub operations: Vec<ProjectEditOperation>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectEditOperation {
    Rename {
        name: String,
    },
    SettingsSet {
        settings: SettingsProfile,
    },
    /// Set source artwork, or null to restore automatic artwork. Assets must
    /// belong to the proposed project; asset_bind may precede this atomically.
    CoverSet {
        cover: Option<ProjectCover>,
    },
    DefaultSequenceSet {
        sequence_id: String,
    },
    AssetBind {
        id: String,
        name: String,
        source: AssetSelector,
    },
    AssetDelete {
        asset_id: String,
    },
    FontBind {
        id: String,
        source: FontSelector,
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
        caption: Box<Caption>,
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
    Bookmark {
        operation: BookmarkOperation,
    },
}
