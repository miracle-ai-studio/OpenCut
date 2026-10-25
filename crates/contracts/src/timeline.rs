//! Fixed-head media editing intents. Source clocks and ripple are planned in
//! application; the returned transaction uses the existing V4 public apply.
use crate::{Time, project_public::ApplyProject, scene::SceneTrackKind};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimelineEditRequest {
    pub project_id: String,
    pub expected_revision: String,
    pub sequence_id: String,
    pub idempotency_key: String,
    pub label: String,
    pub command: TimelineCommand,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimelineTarget {
    pub track_id: String,
    pub element_id: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimelineMove {
    pub target: TimelineTarget,
    pub target_track_id: String,
    /// Requested scene-time start; application performs the frame selection.
    pub new_start: Time,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimelineSplit {
    pub target: TimelineTarget,
    /// Explicit stable right identity. Nested identities are derived by the
    /// planner and checked against the complete project namespace.
    pub right_element_id: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TimelineTrimEdge {
    Left,
    Right,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TimelineSplitRetain {
    Both,
    Left,
    Right,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TimelineCommand {
    MoveSelection {
        moves: Vec<TimelineMove>,
        ripple: bool,
    },
    Trim {
        target: TimelineTarget,
        edge: TimelineTrimEdge,
        boundary: Time,
        ripple: bool,
    },
    Split {
        targets: Vec<TimelineSplit>,
        at: Time,
        retain: TimelineSplitRetain,
        ripple: bool,
    },
    /// Classic closes freed intervals per track, not a scene-wide interval.
    Delete {
        targets: Vec<TimelineTarget>,
        ripple: bool,
    },
    /// Creates an empty non-main media track. Other kinds remain a later matrix.
    AddTrack {
        id: String,
        name: String,
        track_kind: SceneTrackKind,
    },
    DeleteEmptyTrack {
        track_id: String,
    },
    SetTrackLocked {
        track_id: String,
        locked: bool,
    },
    SetTrackEnabled {
        track_id: String,
        enabled: bool,
    },
    SetTrackMuted {
        track_id: String,
        muted: bool,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimelineDecision {
    pub created_ids: Vec<String>,
    pub changed_ids: Vec<String>,
    pub deleted_ids: Vec<String>,
    pub selection: Vec<TimelineTarget>,
    pub ripple_shifted_ids: Vec<String>,
    /// Dependent transitions explicitly removed by the edit, never left dangling.
    pub removed_transition_ids: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimelineEditPlan {
    /// Informational shared canonical request digest; it is not a mutation
    /// receipt. Public apply binds the exact returned transaction independently.
    pub original_request_digest: String,
    pub decision: TimelineDecision,
    pub transaction: Option<ApplyProject>,
}
