//! Versioned observations for complete normal media/free-text frames. These
//! wire values contain no private path and never grant access to font bytes.
use crate::{Time, project_v4::FontStyleV4, visual_frame_v4::*};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextFrameIdentityV2 {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub time: Time,
    pub width: u32,
    pub height: u32,
    pub profile: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextRasterTransformV2 {
    pub center_x: f64,
    pub center_y: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub rotation_radians: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextRasterBackgroundV2 {
    pub enabled: bool,
    pub color: [f64; 4],
    pub padding_x_px: f64,
    pub padding_y_px: f64,
    pub offset_x_px: f64,
    pub offset_y_px: f64,
    pub corner_radius_percent: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextRasterInputV2 {
    pub content: String,
    pub font_size_px: f64,
    pub line_height_px: f64,
    pub letter_spacing_px: f64,
    pub align: String,
    pub decoration: String,
    pub color: [f64; 4],
    pub background: TextRasterBackgroundV2,
    pub transform: TextRasterTransformV2,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextFontResourceV2 {
    pub font_id: String,
    pub content_hash: String,
    pub size: String,
    pub weight: u16,
    pub style: FontStyleV4,
    pub face_index: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValidatedTextFontV2 {
    pub font_id: String,
    pub content_hash: String,
    pub size: String,
    pub weight: u16,
    pub style: FontStyleV4,
    pub face_index: u32,
    pub glyph_coverage_validated: bool,
    pub validated_text_hash: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextLayerPlanV2 {
    pub element_id: String,
    pub text: TextRasterInputV2,
    pub font: ValidatedTextFontV2,
    pub opacity: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextBlockMetricsV2 {
    pub max_width: f64,
    pub height: f64,
    pub visual_center_offset: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextRectV2 {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextBackgroundRectV2 {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
    pub radius: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextLineMetricsV2 {
    pub content: String,
    pub baseline_y: f64,
    pub width: f64,
    pub ascent: f64,
    pub descent: f64,
    pub decoration: Option<TextRectV2>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextLayoutV2 {
    pub font_id: String,
    pub font_content_hash: String,
    pub validated_text_hash: String,
    pub font_weight: u16,
    pub font_style: FontStyleV4,
    pub face_index: u32,
    pub font_size_px: f64,
    pub line_height_px: f64,
    pub letter_spacing_px: f64,
    pub align: String,
    pub baseline: String,
    pub block: TextBlockMetricsV2,
    pub background: Option<TextBackgroundRectV2>,
    pub lines: Vec<TextLineMetricsV2>,
    pub transform: TextRasterTransformV2,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextRasterReceiptV2 {
    pub element_id: String,
    pub width: u32,
    pub height: u32,
    pub layout: TextLayoutV2,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BoundTextFrameV2 {
    pub identity: TextFrameIdentityV2,
    pub clear: [f32; 4],
    pub background: [f32; 4],
    pub layers: Vec<BoundVisualLayerV4>,
    pub decoded: Vec<DecodedVisualSourceV4>,
    pub rasters: Vec<TextRasterReceiptV2>,
}

/// Font resources remain separate from media locks and never expose a CAS path.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontLockV2 {
    pub font_id: String,
    pub content_hash: String,
    pub size: u64,
}
