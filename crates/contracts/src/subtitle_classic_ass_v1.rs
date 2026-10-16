//! Classic ASS compatibility import. The strict external ASS profile is frozen;
//! this operation preserves Classic's mapped styles, fallback and loss warnings.
use crate::{
    project_v4_public::ApplyPublicV4Project,
    subtitle_ass_v1::{SubtitleAssFontBindingV1, SubtitleAssFontRequirementV1},
    subtitle_text_v1::SubtitleTextObservationV1,
    text_authoring_v1::TextTrackTargetV1,
    timeline_v4::TimelineTargetV4,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubtitleClassicAssPlanRequestV1 {
    pub project_id: String,
    pub expected_revision: String,
    pub sequence_id: String,
    pub idempotency_key: String,
    pub track: TextTrackTargetV1,
    pub content: String,
    pub font_bindings: Vec<SubtitleAssFontBindingV1>,
    pub observations: Option<Vec<SubtitleTextObservationV1>>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SubtitleClassicVerticalAlignV1 {
    Top,
    Middle,
    Bottom,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubtitleClassicPlacementV1 {
    pub left_margin_px: f64,
    pub right_margin_px: f64,
    pub vertical_margin_px: f64,
    pub vertical_align: SubtitleClassicVerticalAlignV1,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubtitleClassicAssCuePlanV1 {
    pub timeline_range: crate::TimeRange,
    pub layer: crate::text_frame_v2::TextLayerPlanV2,
    pub directive: crate::text_frame_v3::CaptionRasterPlanV3,
    pub placement: SubtitleClassicPlacementV1,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubtitleClassicAssPlanResultV1 {
    pub schema_version: u32,
    pub revision: String,
    pub width: u32,
    pub height: u32,
    pub font_requirements: Vec<SubtitleAssFontRequirementV1>,
    pub cues: Vec<SubtitleClassicAssCuePlanV1>,
    pub transaction: Option<ApplyPublicV4Project>,
    pub selection: Option<TimelineTargetV4>,
    pub warnings: Vec<String>,
    pub skipped_cue_count: u32,
}
