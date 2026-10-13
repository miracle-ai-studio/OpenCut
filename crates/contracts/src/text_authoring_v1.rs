//! Fixed-head title authoring plans; all writes use the existing V4 transaction.
use crate::{
    Time, TimeRange, project_v4::TextV4, project_v4_public::ApplyPublicV4Project,
    timeline_v4::TimelineTargetV4,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TextTimeInputV1 {
    Exact {
        time: Time,
    },
    /// Application parses bounded decimal seconds exactly; no client tick math.
    DecimalSeconds {
        value: String,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TextValuesV1 {
    pub name: String,
    pub text: TextV4,
    pub start: TextTimeInputV1,
    pub duration: TextTimeInputV1,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TextTrackTargetV1 {
    Existing { track_id: String },
    New { track_id: String },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TextPlanCommandV1 {
    Defaults {
        font_id: String,
        start: TextTimeInputV1,
    },
    Create {
        element_id: String,
        track: TextTrackTargetV1,
        values: TextValuesV1,
    },
    Edit {
        target: TimelineTargetV4,
        values: TextValuesV1,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TextPlanRequestV1 {
    pub project_id: String,
    pub expected_revision: String,
    pub sequence_id: String,
    pub idempotency_key: String,
    pub command: TextPlanCommandV1,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TextInitialV1 {
    pub name: String,
    pub text: TextV4,
    pub timeline_range: TimeRange,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TextPlanResultV1 {
    pub schema_version: u32,
    pub revision: String,
    pub initial: Option<TextInitialV1>,
    pub transaction: Option<ApplyPublicV4Project>,
    pub selection: Option<TimelineTargetV4>,
}
