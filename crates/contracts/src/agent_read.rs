//! Bounded V4 reads for external editing clients. No private resource locators.
use crate::{AssetKind, AssetVersion, Time, TimeRange, scene::*, scene_bookmarks::SceneBookmark};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectSummaryRequest {
    pub project_id: String,
    pub revision: Option<String>,
    pub sequence_offset: u32,
    pub limit: u16,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectSummaryPage {
    pub project_id: String,
    pub revision: String,
    pub name: String,
    pub default_sequence_id: String,
    pub settings: SettingsProfile,
    pub asset_count: usize,
    pub font_count: usize,
    pub sequence_count: usize,
    pub sequences: Vec<SequenceSummaryPage>,
    pub next_sequence_offset: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SequenceSummaryPage {
    pub sequence_id: String,
    pub name: String,
    pub main_track_id: String,
    /// Extent of authored elements and captions, including disabled content.
    /// Bookmarks do not extend the content duration. This is not render admission.
    pub authored_duration: Time,
    pub track_count: usize,
    pub element_count: usize,
    pub caption_count: usize,
    pub transition_count: usize,
    pub bookmark_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimelineQueryRequest {
    pub project_id: String,
    /// Every page belongs to this immutable revision under current read authority.
    pub revision: String,
    pub sequence_id: String,
    /// Positive half-open sequence-time interval; returned objects keep original ranges.
    pub range: TimeRange,
    pub track_ids: Vec<String>,
    pub include_annotations: bool,
    pub offset: u32,
    pub limit: u16,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimelineQuery {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub range: TimeRange,
    /// Stable order: track/element document order, captions, then bookmarks.
    pub items: Vec<TimelineItem>,
    pub matching_items: usize,
    pub next_offset: Option<u32>,
    /// Only owners of objects on this page. Channels retain their original clocks.
    pub tracks: Vec<TimelineTrack>,
    /// Original transitions incident to an element on this page, not cropped copies.
    pub related_transitions: Vec<SceneTransition>,
    pub asset_versions: Vec<AgentAssetVersion>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TimelineItem {
    Element {
        track_id: String,
        element: Box<TimelineElement>,
    },
    Caption {
        caption: Box<Caption>,
    },
    Bookmark {
        bookmark: SceneBookmark,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimelineTrack {
    pub id: String,
    pub name: String,
    pub kind: SceneTrackKind,
    pub enabled: bool,
    pub muted: bool,
    pub locked: bool,
    pub opacity: f64,
    pub gain: Gain,
    pub channels: Vec<AnimationChannel>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AgentAssetVersion {
    pub asset_id: String,
    pub kind: AssetKind,
    pub version: AssetVersion,
}
