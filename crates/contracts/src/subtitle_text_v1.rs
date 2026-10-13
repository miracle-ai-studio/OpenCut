//! Editable subtitles use ordinary TextV4 elements and the existing atomic edit.
//! Metrics are observations; the application owns wrapping, timing and placement.
use crate::{
    TimeRange,
    project_v4::TextV4,
    project_v4_public::ApplyPublicV4Project,
    subtitle_v4::SubtitleFormatV4,
    text_authoring_v1::TextTrackTargetV1,
    text_frame_v2::{TextLayerPlanV2, TextRasterReceiptV2},
    text_frame_v3::{CaptionLayoutReceiptV3, CaptionRasterPlanV3},
    timeline_v4::TimelineTargetV4,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SubtitleTextSourceV1 {
    Transcript {
        analysis_job_id: String,
        words_per_caption: u8,
    },
    File {
        format: SubtitleFormatV4,
        content: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubtitleTextObservationV1 {
    pub caption: CaptionLayoutReceiptV3,
    pub raster: TextRasterReceiptV2,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubtitleTextPlanRequestV1 {
    pub project_id: String,
    pub expected_revision: String,
    pub sequence_id: String,
    pub idempotency_key: String,
    pub track: TextTrackTargetV1,
    pub source: SubtitleTextSourceV1,
    /// Content must be empty. Cue content comes exclusively from the source.
    pub style: TextV4,
    /// None selects the Classic 80% canvas width. Placement uses its 5% bottom margin.
    pub max_width_px: Option<u32>,
    /// None prepares measurement inputs. Some validates them and returns one edit.
    pub observations: Option<Vec<SubtitleTextObservationV1>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubtitleTextCuePlanV1 {
    pub timeline_range: TimeRange,
    pub layer: TextLayerPlanV2,
    pub directive: CaptionRasterPlanV3,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubtitleTextPlanResultV1 {
    pub schema_version: u32,
    pub revision: String,
    pub width: u32,
    pub height: u32,
    pub cues: Vec<SubtitleTextCuePlanV1>,
    pub transaction: Option<ApplyPublicV4Project>,
    pub selection: Option<TimelineTargetV4>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubtitleTextExportV1 {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    /// Explicit scope prevents ordinary titles on other tracks entering subtitles.
    pub track_id: String,
    pub format: SubtitleFormatV4,
}
