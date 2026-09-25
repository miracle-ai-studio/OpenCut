//! Portable compiled visual graph for the schema 3 FFmpeg adapter.
//! Project IDs and effects have no native paths or process configuration.
use crate::MediaTime;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompiledVisualGraphV3 {
    pub targets: Vec<VisualTargetBindingV3>,
    pub shapes: Vec<CompiledShapeV3>,
    pub effects: Vec<EffectNodeV3>,
    pub masks: Vec<MaskNodeV3>,
    pub transitions: Vec<TransitionNodeV3>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VisualTargetBindingV3 {
    pub target_id: String,
    pub kind: VisualTargetKindV3,
    pub index: usize,
    pub z_index: i32,
    pub layer_order: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VisualTargetKindV3 {
    VideoClip,
    ImageClip,
    Shape,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RgbaV3 {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompiledShapeV3 {
    pub id: String,
    pub start: MediaTime,
    pub duration: MediaTime,
    pub primitive: ShapePrimitiveV3,
    pub fill: RgbaV3,
    pub position_x: f64,
    pub position_y: f64,
    pub rotation_degrees: f64,
    pub opacity: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ShapePrimitiveV3 {
    Rectangle { width_px: f64, height_px: f64 },
    Ellipse { radius_x_px: f64, radius_y_px: f64 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EffectNodeV3 {
    pub target_id: String,
    pub effect: VisualEffectV3,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum VisualEffectV3 {
    ColorAdjust {
        exposure_ev: f64,
        contrast: f64,
        saturation: f64,
    },
    GaussianBlur {
        radius_px: f64,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MaskNodeV3 {
    pub target_id: String,
    pub shape: MaskShapeV3,
    pub inverted: bool,
    pub feather: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MaskShapeV3 {
    Rectangle {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    },
    Ellipse {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransitionNodeV3 {
    pub outgoing_clip_id: String,
    pub incoming_clip_id: String,
    pub duration: MediaTime,
    pub transition: TransitionV3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TransitionV3 {
    Crossfade,
}
