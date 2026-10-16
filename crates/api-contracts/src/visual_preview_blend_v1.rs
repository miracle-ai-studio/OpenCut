//! Final blend binding on the same authorized consumer clock and observations.
use crate::{
    visual_blend_frame_v1::ClassicBlendFramePlanV1,
    visual_preview_v4::{VisualDecodeAdmission, VisualPreviewAdmissionV4},
};
use opencut_contracts::{
    blend_frame_v1::BoundBlendFrameV1, graphic_frame_v1::BoundGraphicFrameV1,
    text_frame_v2::BoundTextFrameV2, visual_frame_v4::BoundVisualFrameV4,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub use crate::visual_preview_graphic_v1::VisualPreviewGraphicFrameV1 as VisualPreviewBlendFrameV1;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VisualPreviewBlendFrameResultV1 {
    pub schema_version: u32,
    pub plan: ClassicBlendFramePlanV1,
    pub admission: VisualPreviewAdmissionV4,
    pub decode_admissions: BTreeMap<String, VisualDecodeAdmission>,
    pub bound_media: Option<BoundVisualFrameV4>,
    pub bound_execution: Option<BoundTextFrameV2>,
    pub bound_graphic: Option<BoundGraphicFrameV1>,
    pub bound: Option<BoundBlendFrameV1>,
    pub time_seconds: f64,
    pub consumer_time_seconds: f64,
    pub max_lag_ms: u32,
}
