//! Versioned observations for complete normal media/free-text frames. These
//! wire values contain no private path and never grant access to font bytes.
use crate::{Time, scene::FontStyle, visual_frame::*};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextFrameIdentity {
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
pub struct TextRasterTransform {
    pub center_x: f64,
    pub center_y: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub rotation_radians: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextRasterBackground {
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
pub struct TextRasterInput {
    pub content: String,
    pub font_size_px: f64,
    pub line_height_px: f64,
    pub letter_spacing_px: f64,
    pub align: String,
    pub decoration: String,
    pub color: [f64; 4],
    pub background: TextRasterBackground,
    pub transform: TextRasterTransform,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextFontResource {
    pub font_id: String,
    pub content_hash: String,
    pub size: String,
    pub weight: u16,
    pub style: FontStyle,
    pub face_index: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValidatedTextFont {
    pub font_id: String,
    pub content_hash: String,
    pub size: String,
    pub weight: u16,
    pub style: FontStyle,
    pub face_index: u32,
    pub glyph_coverage_validated: bool,
    pub validated_text_hash: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextLayerPlan {
    pub element_id: String,
    pub text: TextRasterInput,
    pub font: ValidatedTextFont,
    pub opacity: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextBlockMetrics {
    pub max_width: f64,
    pub height: f64,
    pub visual_center_offset: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextRect {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextBackgroundRect {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
    pub radius: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextLineMetrics {
    pub content: String,
    pub baseline_y: f64,
    pub width: f64,
    pub ascent: f64,
    pub descent: f64,
    pub decoration: Option<TextRect>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextLayout {
    pub font_id: String,
    pub font_content_hash: String,
    pub validated_text_hash: String,
    pub font_weight: u16,
    pub font_style: FontStyle,
    pub face_index: u32,
    pub font_size_px: f64,
    pub line_height_px: f64,
    pub letter_spacing_px: f64,
    pub align: String,
    pub baseline: String,
    pub block: TextBlockMetrics,
    pub background: Option<TextBackgroundRect>,
    pub lines: Vec<TextLineMetrics>,
    pub transform: TextRasterTransform,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextRasterReceipt {
    pub element_id: String,
    pub width: u32,
    pub height: u32,
    pub layout: TextLayout,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BoundTextFrame {
    pub identity: TextFrameIdentity,
    pub clear: [f32; 4],
    pub background: [f32; 4],
    pub layers: Vec<BoundVisualLayer>,
    pub decoded: Vec<DecodedVisualSource>,
    pub rasters: Vec<TextRasterReceipt>,
}

/// Font resources remain separate from media locks and never expose a CAS path.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontLock {
    pub font_id: String,
    pub content_hash: String,
    pub size: u64,
}
