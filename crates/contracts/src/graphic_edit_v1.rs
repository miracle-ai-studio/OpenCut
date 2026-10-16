//! Fixed-head parametric graphics authoring. Writes use the existing V4 transaction.
use crate::{
    TimeRange,
    project_v4::{GraphicV4, TransformV4},
    project_v4_public::ApplyPublicV4Project,
    timeline_v4::TimelineTargetV4,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// Reuse the established exact/decimal authoring time contract and parser.
pub use crate::text_authoring_v1::TextTimeInputV1 as GraphicTimeInputV1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GraphicKindV1 {
    Rectangle,
    Ellipse,
    Polygon,
    Star,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphicValuesV1 {
    pub name: String,
    pub graphic: GraphicV4,
    pub transform: TransformV4,
    pub opacity: f64,
    pub start: GraphicTimeInputV1,
    pub duration: GraphicTimeInputV1,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GraphicTrackTargetV1 {
    Existing { track_id: String },
    New { track_id: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GraphicPlanCommandV1 {
    Defaults {
        geometry: GraphicKindV1,
        start: GraphicTimeInputV1,
    },
    Create {
        element_id: String,
        track: GraphicTrackTargetV1,
        values: GraphicValuesV1,
    },
    Edit {
        target: TimelineTargetV4,
        values: GraphicValuesV1,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphicPlanRequestV1 {
    pub project_id: String,
    pub expected_revision: String,
    pub sequence_id: String,
    pub idempotency_key: String,
    pub command: GraphicPlanCommandV1,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphicInitialV1 {
    pub name: String,
    pub graphic: GraphicV4,
    pub transform: TransformV4,
    pub opacity: f64,
    pub timeline_range: TimeRange,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphicPlanResultV1 {
    pub schema_version: u32,
    pub revision: String,
    pub initial: Option<GraphicInitialV1>,
    pub transaction: Option<ApplyPublicV4Project>,
    pub selection: Option<TimelineTargetV4>,
}
