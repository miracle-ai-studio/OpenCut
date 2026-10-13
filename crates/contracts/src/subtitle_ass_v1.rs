//! Style-preserving ASS authoring into ordinary TextV4 elements.
use crate::{
    TimeRange,
    project_v4::{FontStyleV4, FontWeightV4},
    project_v4_public::ApplyPublicV4Project,
    text_authoring_v1::TextTrackTargetV1,
    text_frame_v2::{TextLayerPlanV2, TextRasterReceiptV2},
    timeline_v4::TimelineTargetV4,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubtitleAssFontBindingV1 {
    pub family: String,
    pub weight: FontWeightV4,
    pub style: FontStyleV4,
    pub font_id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubtitleAssFontRequirementV1 {
    pub family: String,
    pub weight: FontWeightV4,
    pub style: FontStyleV4,
    pub resolved_font_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubtitleAssPlanRequestV1 {
    pub project_id: String,
    pub expected_revision: String,
    pub sequence_id: String,
    pub idempotency_key: String,
    pub track: TextTrackTargetV1,
    pub content: String,
    /// Explicit aliases; otherwise exact pinned family/weight/style is required.
    pub font_bindings: Vec<SubtitleAssFontBindingV1>,
    pub observations: Option<Vec<TextRasterReceiptV2>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubtitleAssCuePlanV1 {
    pub timeline_range: TimeRange,
    pub layer: TextLayerPlanV2,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubtitleAssPlanResultV1 {
    pub schema_version: u32,
    pub revision: String,
    pub width: u32,
    pub height: u32,
    /// Unresolved fonts yield no cues or transaction; callers may provide aliases.
    pub font_requirements: Vec<SubtitleAssFontRequirementV1>,
    pub cues: Vec<SubtitleAssCuePlanV1>,
    pub transaction: Option<ApplyPublicV4Project>,
    pub selection: Option<TimelineTargetV4>,
    pub warnings: Vec<String>,
}
