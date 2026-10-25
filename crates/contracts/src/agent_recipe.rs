//! Deterministic recipes compile to ordinary public edits; no model calls.
use crate::{
    Time, TimeRange,
    agent_edit::AgentEditPlan,
    media_import::{MediaBinding, MediaPlacement, MediaTrack},
    scene::*,
    timeline::TimelineTarget,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecipeRequest {
    pub project_id: String,
    pub expected_revision: String,
    pub sequence_id: String,
    pub idempotency_key: String,
    pub label: String,
    pub recipe: Recipe,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Recipe {
    /// Sequence intervals are explicit editorial decisions; ripple follows the
    /// existing per-track planner, so sync groups must include every placement.
    JumpCut {
        targets: Vec<TimelineTarget>,
        ranges: Vec<TimeRange>,
        ripple: bool,
    },
    /// Silence or transcript evidence stays linked to a fixed source asset.
    RemoveEvidence {
        target: TimelineTarget,
        analysis_job_id: String,
        evidence_ids: Vec<String>,
        ripple: bool,
    },
    /// Shared media planner supports B-roll and music placement without rebinding
    /// existing project assets or inventing a second insertion algorithm.
    PlaceMedia {
        bindings: Vec<MediaBinding>,
        placements: Vec<MediaPlacement>,
        new_tracks: Vec<MediaTrack>,
    },
    DuckMusic {
        targets: Vec<TimelineTarget>,
        speech_ranges: Vec<TimeRange>,
        duck_decibels: f64,
        ramp: Time,
        replace_gain_automation: bool,
    },
    /// Explicit user/Agent framing samples; no implicit face/subject detector.
    Reframe {
        canvas: CanvasSize,
        placements: Vec<RecipeFraming>,
    },
    Captions {
        analysis_job_id: String,
        track_id: String,
        words_per_caption: u8,
        style: SceneCaptionStyle,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecipeFraming {
    pub target: TimelineTarget,
    pub samples: Vec<RecipeTransformSample>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecipeTransformSample {
    pub local_time: Time,
    pub transform: Transform,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecipePlan {
    pub original_request_digest: String,
    pub affected_elements: Vec<String>,
    pub evidence_ids: Vec<String>,
    pub warnings: Vec<String>,
    /// Dry-run only; one original-key V4 public transaction is the only write.
    pub plan: Option<AgentEditPlan>,
}
