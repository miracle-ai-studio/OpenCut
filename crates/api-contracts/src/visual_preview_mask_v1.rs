//! Masks bind to the original authorized sound session and fixed observations.
pub use crate::visual_preview_blend_v1::VisualPreviewBlendFrameV1 as VisualPreviewMaskFrameV1;
use crate::{
    visual_mask_frame_v1::ClassicMaskFramePlanV1,
    visual_preview_v4::{VisualDecodeAdmission, VisualPreviewAdmissionV4},
};
use opencut_contracts::{
    blend_frame_v1::BoundBlendFrameV1, graphic_frame_v1::BoundGraphicFrameV1,
    mask_frame_v1::BoundMaskFrameV1, text_frame_v2::BoundTextFrameV2,
    visual_frame_v4::BoundVisualFrameV4,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VisualPreviewMaskFrameResultV1 {
    pub schema_version: u32,
    pub plan: ClassicMaskFramePlanV1,
    pub admission: VisualPreviewAdmissionV4,
    pub decode_admissions: BTreeMap<String, VisualDecodeAdmission>,
    pub bound_media: Option<BoundVisualFrameV4>,
    pub bound_execution: Option<BoundTextFrameV2>,
    pub bound_graphic: Option<BoundGraphicFrameV1>,
    pub bound_blend: Option<BoundBlendFrameV1>,
    pub bound: Option<BoundMaskFrameV1>,
    pub time_seconds: f64,
    pub consumer_time_seconds: f64,
    pub max_lag_ms: u32,
}
