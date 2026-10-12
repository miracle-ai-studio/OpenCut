//! Fixed-head media editing intents. Source clocks and ripple are planned in
//! application; the returned transaction uses the existing V4 public apply.
use crate::{Time, project_v4::TrackKindV4, project_v4_public::ApplyPublicV4Project};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimelineEditRequestV4 {
    pub project_id: String,
    pub expected_revision: String,
    pub sequence_id: String,
    pub idempotency_key: String,
    pub label: String,
    pub command: TimelineCommandV4,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimelineTargetV4 {
    pub track_id: String,
    pub element_id: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimelineMoveV4 {
    pub target: TimelineTargetV4,
    pub target_track_id: String,
    /// Requested scene-time start; application performs the frame selection.
    pub new_start: Time,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimelineSplitV4 {
    pub target: TimelineTargetV4,
    /// Explicit stable right identity. Nested identities are derived by the
    /// planner and checked against the complete project namespace.
    pub right_element_id: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TimelineTrimEdgeV4 {
    Left,
    Right,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TimelineSplitRetainV4 {
    Both,
    Left,
    Right,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TimelineCommandV4 {
    MoveSelection {
        moves: Vec<TimelineMoveV4>,
        ripple: bool,
    },
    Trim {
        target: TimelineTargetV4,
        edge: TimelineTrimEdgeV4,
        boundary: Time,
        ripple: bool,
    },
    Split {
        targets: Vec<TimelineSplitV4>,
        at: Time,
        retain: TimelineSplitRetainV4,
        ripple: bool,
    },
    /// Classic closes freed intervals per track, not a scene-wide interval.
    Delete {
        targets: Vec<TimelineTargetV4>,
        ripple: bool,
    },
    /// Creates an empty non-main media track. Other kinds remain a later matrix.
    AddTrack {
        id: String,
        name: String,
        track_kind: TrackKindV4,
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
pub struct TimelineDecisionV4 {
    pub created_ids: Vec<String>,
    pub changed_ids: Vec<String>,
    pub deleted_ids: Vec<String>,
    pub selection: Vec<TimelineTargetV4>,
    pub ripple_shifted_ids: Vec<String>,
    /// Dependent transitions explicitly removed by the edit, never left dangling.
    pub removed_transition_ids: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimelineEditPlanV4 {
    /// Informational shared canonical request digest; it is not a mutation
    /// receipt. Public apply binds the exact returned transaction independently.
    pub original_request_digest: String,
    pub decision: TimelineDecisionV4,
    pub transaction: Option<ApplyPublicV4Project>,
}
