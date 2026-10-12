//! Ephemeral picture planning/binding attached to an existing sound owner lease.
use crate::audio_preview_v4::AudioPreviewStreamV4;
use opencut_contracts::{Time, visual_frame_v4::*};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VisualResourceKind {
    Video,
    Image,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VisualResource {
    pub asset_id: String,
    pub content_hash: String,
    pub size: String,
    pub kind: VisualResourceKind,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClassicVisualLayerPlan {
    pub element_id: String,
    pub resource: VisualResource,
    pub decode: VisualDecodeRequest,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum VisualDecodeRequest {
    Image,
    Video {
        #[serde(rename = "requestedTime")]
        requested_time: Time,
        /// Decoder API projection supplied by Rust; the browser never divides
        /// cross-process integer ticks or applies retime itself.
        #[serde(rename = "requestedTimeSeconds")]
        requested_time_seconds: f64,
        #[serde(rename = "pointProfile")]
        point_profile: String,
        #[serde(rename = "selectedVideoStream")]
        selected_video_stream: Option<u32>,
        #[serde(rename = "noFrameReason")]
        no_frame_reason: Option<VisualNoFrameReasonV4>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClassicVisualPlan {
    pub identity: VisualFrameIdentityV4,
    pub resources: Vec<VisualResource>,
    pub layers: Vec<ClassicVisualLayerPlan>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VisualDecodeAdmission {
    pub max_width: u32,
    pub max_height: u32,
    pub max_pixels: String,
    pub max_decoded_bytes: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VisualPreviewFrameV4 {
    pub session: AudioPreviewStreamV4,
    /// None plans the current confirmed consumer time. Bind uses that exact Time.
    pub time: Option<Time>,
    pub decoded: Option<Vec<DecodedVisualSourceV4>>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VisualPreviewAdmissionV4 {
    pub max_frame_pixels: String,
    pub max_decoded_pixels: String,
    pub max_gpu_bytes: String,
    pub max_rgba_bytes: String,
    pub max_encoded_bytes: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VisualPreviewFrameResultV4 {
    pub schema_version: u32,
    pub plan: ClassicVisualPlan,
    pub admission: VisualPreviewAdmissionV4,
    pub decode_admissions: BTreeMap<String, VisualDecodeAdmission>,
    pub bound: Option<BoundVisualFrameV4>,
    /// Presentation projection only. No client-side source clock/layout evaluation.
    pub time_seconds: f64,
    pub consumer_time_seconds: f64,
    pub max_lag_ms: u32,
}
