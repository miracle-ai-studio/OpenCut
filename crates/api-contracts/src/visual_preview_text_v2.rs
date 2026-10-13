//! Additive text-capable picture binding on the original audio owner lease.
use crate::{
    audio_preview_v4::AudioPreviewStreamV4,
    visual_preview_v4::{VisualDecodeAdmission, VisualPreviewAdmissionV4},
    visual_text_frame_v2::ClassicTextFramePlanV2,
};
use opencut_contracts::{
    Time,
    text_frame_v2::{BoundTextFrameV2, TextRasterReceiptV2},
    visual_frame_v4::{BoundVisualFrameV4, DecodedVisualSourceV4},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VisualPreviewTextFrameV2 {
    pub session: AudioPreviewStreamV4,
    pub time: Option<Time>,
    pub decoded: Option<Vec<DecodedVisualSourceV4>>,
    pub rasters: Option<Vec<TextRasterReceiptV2>>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VisualPreviewTextFrameResultV2 {
    pub schema_version: u32,
    pub plan: ClassicTextFramePlanV2,
    pub admission: VisualPreviewAdmissionV4,
    pub decode_admissions: BTreeMap<String, VisualDecodeAdmission>,
    pub bound_media: Option<BoundVisualFrameV4>,
    pub bound: Option<BoundTextFrameV2>,
    pub time_seconds: f64,
    pub consumer_time_seconds: f64,
    pub max_lag_ms: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VisualPreviewFontV2 {
    pub session: AudioPreviewStreamV4,
    pub font_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VisualPreviewFontAuthorizedV2 {
    pub schema_version: u32,
    pub font_id: String,
}
