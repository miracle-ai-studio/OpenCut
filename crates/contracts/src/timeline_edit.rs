//! Reviewed editorial commands lower to the existing atomic visual transaction.
use crate::{Time, TimeRange, historical_visual_ops::ApplyHistoricalVisualProject};
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TimelineCommand {
    /// Remove a sequence interval across all tracks, captions and shapes.
    RippleDelete { range: TimeRange },
    /// Insert sequence time across all tracks, splitting clips at the boundary.
    InsertGap { at: Time, duration: Time },
    /// Shift the source window without moving the clip or its animation clock.
    Slip { clip_id: String, source_delta: Time },
    /// Move the cut between adjacent clips, preserving their outside boundaries.
    Roll {
        outgoing_clip_id: String,
        incoming_clip_id: String,
        delta: Time,
    },
    /// Move a clip between adjacent neighbors while trimming both neighbors.
    Slide { clip_id: String, delta: Time },
    /// Move an explicit sync selection in one commit; locked tracks reject it.
    MoveGroup {
        clip_ids: Vec<String>,
        caption_ids: Vec<String>,
        shape_ids: Vec<String>,
        delta: Time,
    },
    /// A complete permutation of the sequence's track IDs, back to front.
    ReorderTracks { track_ids: Vec<String> },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimelineEditPlan {
    /// Submit unchanged through the shared visual edit apply operation.
    pub transaction: ApplyHistoricalVisualProject,
    pub created_ids: Vec<String>,
    pub changed_ids: Vec<String>,
    pub deleted_ids: Vec<String>,
    /// Explicit consequences to review, including transitions removed at a cut.
    pub notes: Vec<String>,
}

/// Versioned Classic actions; the original TimelineCommand schema stays frozen.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClassicEditRequest {
    pub project_id: String,
    pub expected_revision: String,
    pub sequence_id: String,
    pub idempotency_key: String,
    pub label: String,
    pub ripple: bool,
    pub command: ClassicCommand,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClassicCommand {
    /// Explicit selection; out-of-range split boundaries leave elements intact.
    Split {
        clip_ids: Vec<String>,
        caption_ids: Vec<String>,
        shape_ids: Vec<String>,
        at: Time,
        retain: SplitRetain,
    },
    /// Plan an explicit visual transaction plus per-track Classic ripple.
    Apply {
        operations: Vec<crate::historical_visual_ops::VisualOperation>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SplitRetain {
    Both,
    Left,
    Right,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClassicSelection {
    pub clip_ids: Vec<String>,
    pub caption_ids: Vec<String>,
    pub shape_ids: Vec<String>,
    pub playhead: Option<Time>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClassicEditPlan {
    pub plan: TimelineEditPlan,
    /// Split's explicit selection override; generic edits have no override.
    pub selection: Option<ClassicSelection>,
}
