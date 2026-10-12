use crate::{AssetVersion, project_v4::*, scene_bookmarks::BookmarkOperationV1};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PublicAssetSelectorV4 {
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
pub enum PublicFontSelectorV4 {
    ProjectFont { revision: String, font_id: String },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApplyPublicV4Project {
    pub project_id: String,
    pub expected_revision: String,
    pub idempotency_key: String,
    pub label: String,
    pub operations: Vec<PublicProjectOperationV4>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum PublicProjectOperationV4 {
    Rename {
        name: String,
    },
    SettingsSet {
        settings: SettingsProfileV4,
    },
    DefaultSequenceSet {
        sequence_id: String,
    },
    AssetBind {
        id: String,
        name: String,
        source: PublicAssetSelectorV4,
    },
    AssetDelete {
        asset_id: String,
    },
    FontBind {
        id: String,
        source: PublicFontSelectorV4,
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
        caption: Box<CaptionV4>,
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
    Bookmark {
        operation: BookmarkOperationV1,
    },
}
