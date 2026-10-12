//! Fixed-revision element clipboard. References are not capabilities: copy and
//! paste recheck live source Read authority, and paste checks the target edit.
use crate::{Time, project_v4::TrackKindV4, project_v4_public::ApplyPublicV4Project};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClipboardElementTargetV4 {
    pub track_id: String,
    pub element_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CopyTimelineSelectionV4 {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub targets: Vec<ClipboardElementTargetV4>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimelineClipboardItemV4 {
    pub source_track_id: String,
    pub source_track_kind: TrackKindV4,
    pub source_element_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimelineClipboardV4 {
    pub schema_version: u32,
    pub source_project_id: String,
    pub source_revision: String,
    pub source_sequence_id: String,
    /// Ordered fixed-revision references, never client-authored node payloads.
    pub items: Vec<TimelineClipboardItemV4>,
    /// Binds full elements and their exact resource records at the source.
    pub selection_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClipboardTrackInsertionV4 {
    /// Relative visual layer order; audio uses its displayed top-to-bottom order.
    Above {
        track_id: String,
    },
    Below {
        track_id: String,
    },
    /// Classic's highest overlay or first audio track, never a new main track.
    Highest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClipboardTrackTargetV4 {
    Existing {
        track_id: String,
    },
    New {
        track_id: String,
        name: String,
        track_kind: TrackKindV4,
        insertion_anchor: ClipboardTrackInsertionV4,
    },
    /// Server resolves the track immediately above the source, then the first
    /// available compatible unlocked track, otherwise creates at Highest.
    ClassicAboveSource {
        /// Reserved even if an existing track is chosen; must be a fresh ID.
        new_track_id: String,
        name: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClipboardElementIdentityV4 {
    pub source_element_id: String,
    pub new_element_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClipboardTrackPlacementV4 {
    pub source_track_id: String,
    pub target: ClipboardTrackTargetV4,
    /// Exactly the copied members of this source track, in clipboard order.
    pub elements: Vec<ClipboardElementIdentityV4>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PasteTimelineSelectionV4 {
    pub project_id: String,
    pub expected_revision: String,
    pub sequence_id: String,
    pub idempotency_key: String,
    pub label: String,
    pub clipboard: TimelineClipboardV4,
    /// Scene time. Paste preserves exact relative offsets and does not perform
    /// media-import FPS/canvas updates or independent per-element frame snaps.
    pub at: Time,
    pub placements: Vec<ClipboardTrackPlacementV4>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClipboardActualStartV4 {
    pub element_id: String,
    pub start: Time,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PasteTimelineDecisionV4 {
    pub selection: Vec<ClipboardElementTargetV4>,
    pub created_ids: Vec<String>,
    pub new_track_ids: Vec<String>,
    pub restored_asset_ids: Vec<String>,
    pub restored_font_ids: Vec<String>,
    pub actual_starts: Vec<ClipboardActualStartV4>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PasteTimelinePlanV4 {
    pub decision: PasteTimelineDecisionV4,
    /// Commit this exact public transaction. This read-only plan is not an
    /// idempotency receipt and never transfers cross-project resource grants.
    pub transaction: ApplyPublicV4Project,
}
