//! Fixed-head parametric graphics authoring through the shared atomic transaction.
use crate::{
    TimeRange,
    project_public::ApplyProject,
    scene::{Graphic, Transform},
    timeline::TimelineTarget,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// Reuse the established exact/decimal authoring time contract and parser.
pub use crate::text_authoring::TextTimeInput as GraphicTimeInput;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GraphicKind {
    Rectangle,
    Ellipse,
    Polygon,
    Star,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphicValues {
    pub name: String,
    pub graphic: Graphic,
    pub transform: Transform,
    pub opacity: f64,
    pub start: GraphicTimeInput,
    pub duration: GraphicTimeInput,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GraphicTrackTarget {
    Existing { track_id: String },
    New { track_id: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GraphicPlanCommand {
    Defaults {
        geometry: GraphicKind,
        start: GraphicTimeInput,
    },
    Create {
        element_id: String,
        track: GraphicTrackTarget,
        values: GraphicValues,
    },
    Edit {
        target: TimelineTarget,
        values: GraphicValues,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphicPlanRequest {
    pub project_id: String,
    pub expected_revision: String,
    pub sequence_id: String,
    pub idempotency_key: String,
    pub command: GraphicPlanCommand,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphicInitial {
    pub name: String,
    pub graphic: Graphic,
    pub transform: Transform,
    pub opacity: f64,
    pub timeline_range: TimeRange,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphicPlanResult {
    pub revision: String,
    pub initial: Option<GraphicInitial>,
    pub transaction: Option<ApplyProject>,
    pub selection: Option<TimelineTarget>,
}
