//! Schema 3 visual graph. Schema 2 project contracts remain unchanged.
use crate::{ProjectDocument, Time, TimeRange};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectDocumentV3 {
    pub schema_version: u32,
    /// The schema 2 project remains the single authority for assets, tracks,
    /// clip timing, audio and captions. Visual nodes refer to its stable IDs.
    pub base: ProjectDocument,
    pub visual: Vec<SequenceVisualV3>,
    /// Versioned easing overrides for existing base keyframe channels.
    #[serde(default)]
    pub keyframe_curves: Vec<KeyframeCurveV1>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeyframeCurveV1 {
    pub id: String,
    pub schema_version: u32,
    pub sequence_id: String,
    pub clip_id: String,
    pub parameter: String,
    /// The outgoing keyframe controls the segment to the next keyframe.
    pub keyframe_id: String,
    pub curve: CubicBezierV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CubicBezierV1 {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectSnapshotV3 {
    pub project: ProjectDocumentV3,
    pub revision: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SequenceVisualV3 {
    pub sequence_id: String,
    pub shapes: Vec<ShapeClip>,
    pub effects: Vec<EffectNode>,
    pub masks: Vec<MaskNode>,
    pub transitions: Vec<TransitionNode>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Rgba {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ShapeClip {
    pub id: String,
    pub track_id: String,
    pub timeline_range: TimeRange,
    pub primitive: ShapePrimitive,
    pub fill: Rgba,
    /// Canvas pixel offset from center, positive right/down.
    pub position_x: f64,
    pub position_y: f64,
    pub rotation_degrees: f64,
    pub opacity: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ShapePrimitive {
    Rectangle { width_px: f64, height_px: f64 },
    Ellipse { radius_x_px: f64, radius_y_px: f64 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EffectNode {
    pub id: String,
    /// A base video/image clip ID or a schema 3 shape ID.
    pub target_id: String,
    pub effect: VisualEffect,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum VisualEffect {
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
pub struct MaskNode {
    pub id: String,
    pub target_id: String,
    /// Coordinates are normalized to the canvas, from its top-left corner.
    pub shape: MaskShape,
    pub inverted: bool,
    pub feather: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MaskShape {
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
pub struct TransitionNode {
    pub id: String,
    pub outgoing_clip_id: String,
    pub incoming_clip_id: String,
    /// The outgoing end must equal the incoming start. Each source must have
    /// half this duration of real media outside its visible source range.
    pub duration: Time,
    pub transition: Transition,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Transition {
    Crossfade,
}
