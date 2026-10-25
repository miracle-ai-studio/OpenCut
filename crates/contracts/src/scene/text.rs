//! Independent titles and captions with one font authority and explicit layout units.
use super::{Color, FontBinding};
use crate::{CaptionSourceAnchor, TimeRange};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Caption {
    pub id: String,
    /// The sole owning caption track in this sequence, providing visibility,
    /// locking and layer order. Promoting legacy sequence-wide captions creates
    /// an explicit caption track at their original overlay position.
    pub track_id: String,
    pub timeline_range: TimeRange,
    pub text: String,
    pub source_anchor: Option<CaptionSourceAnchor>,
    pub style: SceneCaptionStyle,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SceneCaptionStyle {
    pub font: FontBinding,
    pub font_size: FontSize,
    pub color: Color,
    pub background_color: Option<Color>,
    pub max_width_px: Option<u32>,
    pub line_height_px: Option<f32>,
    pub text_align: SceneTextAlign,
    pub layout: CaptionLayoutProfile,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CaptionLayoutProfile {
    LegacyCaptionV2,
    PinnedCaptionV4,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Text {
    pub content: String,
    pub font: FontBinding,
    pub font_weight: FontWeight,
    pub font_style: FontStyle,
    pub decoration: TextDecoration,
    pub align: SceneTextAlign,
    pub font_size: FontSize,
    pub color: Color,
    pub letter_spacing_px: f64,
    pub line_height_multiplier: f64,
    pub background: TextBackground,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FontWeight {
    Normal,
    Bold,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FontStyle {
    Normal,
    Italic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TextDecoration {
    None,
    Underline,
    LineThrough,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SceneTextAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "unit", rename_all = "snake_case", deny_unknown_fields)]
pub enum FontSize {
    /// Actual pixels = value * layout height / 90. Title layout height is the
    /// project canvas height; text-mask layout height is its element height.
    ClassicHeight90 {
        value: f64,
    },
    CanvasPixels {
        value: f64,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TextBackground {
    pub enabled: bool,
    pub color: Color,
    /// Classic 0..100 radius percentage, not a pixel radius.
    pub corner_radius_percent: f64,
    /// Classic padding pixels at font-size 15; actual padding multiplies by
    /// font size / 15. Offsets remain pixels and do not use that multiplier.
    pub padding_x_at_size15: f64,
    pub padding_y_at_size15: f64,
    pub offset_x_px: f64,
    pub offset_y_px: f64,
}
