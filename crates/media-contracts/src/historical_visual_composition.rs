//! Portable compiled visual graph for the schema 3 FFmpeg adapter.
//! Project IDs and effects have no native paths or process configuration.
use crate::MediaTime;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoricalVisualCompiledVisualGraph {
    pub targets: Vec<HistoricalVisualVisualTargetBinding>,
    pub shapes: Vec<HistoricalVisualCompiledShape>,
    pub effects: Vec<HistoricalVisualEffectNode>,
    pub masks: Vec<HistoricalVisualMaskNode>,
    pub transitions: Vec<HistoricalVisualTransitionNode>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoricalVisualVisualTargetBinding {
    pub target_id: String,
    pub kind: HistoricalVisualVisualTargetKind,
    pub index: usize,
    pub z_index: i32,
    pub layer_order: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HistoricalVisualVisualTargetKind {
    VideoClip,
    ImageClip,
    Shape,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoricalVisualRgba {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoricalVisualCompiledShape {
    pub id: String,
    pub start: MediaTime,
    pub duration: MediaTime,
    pub primitive: HistoricalVisualShapePrimitive,
    pub fill: HistoricalVisualRgba,
    pub position_x: f64,
    pub position_y: f64,
    pub rotation_degrees: f64,
    pub opacity: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum HistoricalVisualShapePrimitive {
    Rectangle { width_px: f64, height_px: f64 },
    Ellipse { radius_x_px: f64, radius_y_px: f64 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoricalVisualEffectNode {
    pub target_id: String,
    pub effect: HistoricalVisualVisualEffect,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum HistoricalVisualVisualEffect {
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
pub struct HistoricalVisualMaskNode {
    pub target_id: String,
    pub shape: HistoricalVisualMaskShape,
    pub inverted: bool,
    pub feather: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum HistoricalVisualMaskShape {
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
pub struct HistoricalVisualTransitionNode {
    pub outgoing_clip_id: String,
    pub incoming_clip_id: String,
    pub duration: MediaTime,
    pub transition: HistoricalVisualTransition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HistoricalVisualTransition {
    Crossfade,
}
