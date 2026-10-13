//! Caption observations on the original authorized sound-owner clock.
use crate::{
    audio_preview_v4::AudioPreviewStreamV4,
    visual_preview_v4::{VisualDecodeAdmission, VisualPreviewAdmissionV4},
    visual_text_frame_v3::ClassicTextFramePlanV3,
};
use opencut_contracts::{
    Time,
    text_frame_v2::{BoundTextFrameV2, TextRasterReceiptV2},
    text_frame_v3::CaptionLayoutReceiptV3,
    visual_frame_v4::{BoundVisualFrameV4, DecodedVisualSourceV4},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VisualPreviewCaptionFrameV3 {
    pub session: AudioPreviewStreamV4,
    pub time: Option<Time>,
    pub decoded: Option<Vec<DecodedVisualSourceV4>>,
    pub rasters: Option<Vec<TextRasterReceiptV2>>,
    pub captions: Option<Vec<CaptionLayoutReceiptV3>>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VisualPreviewCaptionFrameResultV3 {
    pub schema_version: u32,
    pub plan: ClassicTextFramePlanV3,
    pub admission: VisualPreviewAdmissionV4,
    pub decode_admissions: BTreeMap<String, VisualDecodeAdmission>,
    pub bound_media: Option<BoundVisualFrameV4>,
    pub bound: Option<BoundTextFrameV2>,
    pub time_seconds: f64,
    pub consumer_time_seconds: f64,
    pub max_lag_ms: u32,
}
