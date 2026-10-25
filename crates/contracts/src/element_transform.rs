//! Sampled gesture geometry and explicit Classic transform intent. Samples are
//! observations of a fixed head; they never replace unrelated authored values.
use crate::{
    Time,
    mask_frame::MaskQuad,
    scene::{CanvasSize, Transform},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TransformGestureIntent {
    Translate,
    Rotate,
    ScaleX,
    ScaleY,
    #[serde(rename = "scale_xy")]
    ScaleXy,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SampleElementTransforms {
    pub project_id: String,
    pub expected_revision: String,
    pub sequence_id: String,
    pub time: Time,
    /// 1..60 distinct active visual element IDs, returned in this exact order.
    pub element_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SampledElementTransform {
    pub element_id: String,
    pub track_id: String,
    pub transform: Transform,
    /// Present for video, image, graphic and sticker. Text uses the separately
    /// admitted Canvas font layout; its transform is still sampled above.
    pub geometry: Option<SampledElementGeometry>,
}

/// Authoring observations from this same fixed frame, without opening media.
/// Actual decoder dimensions remain independently bound by the render pipeline.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SampledElementGeometry {
    /// Upright observed source dimensions (or the graphic's logical source size).
    pub intrinsic_size: CanvasSize,
    /// Contained canvas extent with the sampled transform and opacity applied.
    pub quad: MaskQuad,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SampledElementTransforms {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub time: Time,
    pub elements: Vec<SampledElementTransform>,
}
