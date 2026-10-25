//! Fixed-head title authoring plans; all writes use the existing V4 transaction.
use crate::{Time, TimeRange, project_public::ApplyProject, scene::Text, timeline::TimelineTarget};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TextTimeInput {
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
pub struct TextValues {
    pub name: String,
    pub text: Text,
    pub start: TextTimeInput,
    pub duration: TextTimeInput,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TextTrackTarget {
    Existing { track_id: String },
    New { track_id: String },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TextPlanCommand {
    Defaults {
        font_id: String,
        start: TextTimeInput,
    },
    Create {
        element_id: String,
        track: TextTrackTarget,
        values: TextValues,
    },
    Edit {
        target: TimelineTarget,
        values: TextValues,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TextPlanRequest {
    pub project_id: String,
    pub expected_revision: String,
    pub sequence_id: String,
    pub idempotency_key: String,
    pub command: TextPlanCommand,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TextInitial {
    pub name: String,
    pub text: Text,
    pub timeline_range: TimeRange,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TextPlanResult {
    pub revision: String,
    pub initial: Option<TextInitial>,
    pub transaction: Option<ApplyProject>,
    pub selection: Option<TimelineTarget>,
}
