//! Sampled parametric graphic rasters for the versioned graphic frame executor.
//! Generated sources carry no asset grant and never resolve a native path.
use crate::{
    project_v4::StrokeAlignV4,
    text_frame_v2::{TextFrameIdentityV2, TextRasterReceiptV2},
    text_frame_v3::CaptionLayoutReceiptV3,
    visual_frame_v4::{BoundVisualLayerV4, DecodedVisualSourceV4, VisualQuadV4},
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
pub enum GraphicGeometryPlanV1 {
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
pub struct GraphicStrokePlanV1 {
    pub color: [f64; 4],
    pub width_px: f64,
    pub align: StrokeAlignV4,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GraphicRasterPlanV1 {
    pub geometry: GraphicGeometryPlanV1,
    pub source_width: u32,
    pub source_height: u32,
    pub fill: [f64; 4],
    pub stroke: GraphicStrokePlanV1,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GraphicLayerPlanV1 {
    pub element_id: String,
    pub graphic: GraphicRasterPlanV1,
    /// Uses the logical source's fit scale before adding transparent stroke
    /// padding. None represents an authored zero-sized transform.
    pub quad: Option<VisualQuadV4>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GraphicRasterReceiptV1 {
    pub element_id: String,
    pub width: u32,
    pub height: u32,
    pub origin_x: u32,
    pub origin_y: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BoundGraphicFrameV1 {
    pub identity: TextFrameIdentityV2,
    pub clear: [f32; 4],
    pub background: [f32; 4],
    pub layers: Vec<BoundVisualLayerV4>,
    pub decoded: Vec<DecodedVisualSourceV4>,
    pub rasters: Vec<TextRasterReceiptV2>,
    pub captions: Vec<CaptionLayoutReceiptV3>,
    pub graphics: Vec<GraphicRasterReceiptV1>,
}
