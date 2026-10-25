//! Sampled parametric graphic rasters for the versioned graphic frame executor.
//! Generated sources carry no asset grant and never resolve a native path.
use crate::{
    caption_frame::CaptionLayoutReceipt,
    scene::StrokeAlign,
    text_frame::{TextFrameIdentity, TextRasterReceipt},
    visual_frame::{BoundVisualLayer, DecodedVisualSource, VisualQuad},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum GraphicGeometryPlan {
    Rectangle {
        corner_radius_percent: f64,
    },
    Ellipse,
    Polygon {
        sides: f64,
        corner_radius_percent: f64,
    },
    Star {
        points: f64,
        depth_percent: f64,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GraphicStrokePlan {
    pub color: [f64; 4],
    pub width_px: f64,
    pub align: StrokeAlign,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GraphicRasterPlan {
    pub geometry: GraphicGeometryPlan,
    pub source_width: u32,
    pub source_height: u32,
    pub fill: [f64; 4],
    pub stroke: GraphicStrokePlan,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GraphicLayerPlan {
    pub element_id: String,
    pub graphic: GraphicRasterPlan,
    /// Uses the logical source's fit scale before adding transparent stroke
    /// padding. None represents an authored zero-sized transform.
    pub quad: Option<VisualQuad>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GraphicRasterReceipt {
    pub element_id: String,
    pub width: u32,
    pub height: u32,
    pub origin_x: u32,
    pub origin_y: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BoundGraphicFrame {
    pub identity: TextFrameIdentity,
    pub clear: [f32; 4],
    pub background: [f32; 4],
    pub layers: Vec<BoundVisualLayer>,
    pub decoded: Vec<DecodedVisualSource>,
    pub rasters: Vec<TextRasterReceipt>,
    pub captions: Vec<CaptionLayoutReceipt>,
    pub graphics: Vec<GraphicRasterReceipt>,
}
