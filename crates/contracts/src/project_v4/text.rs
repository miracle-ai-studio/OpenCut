//! Independent titles and captions with one font authority and explicit layout units.
use super::{ColorV4, FontBindingV4};
use crate::{CaptionSourceAnchor, TimeRange};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CaptionV4 {
    pub id: String,
    /// The sole owning caption track in this sequence, providing visibility,
    /// locking and layer order. Promoting legacy sequence-wide captions creates
    /// an explicit caption track at their original overlay position.
    pub track_id: String,
    pub timeline_range: TimeRange,
    pub text: String,
    pub source_anchor: Option<CaptionSourceAnchor>,
    pub style: CaptionStyleV4,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CaptionStyleV4 {
    pub font: FontBindingV4,
    pub font_size: FontSizeV4,
    pub color: ColorV4,
    pub background_color: Option<ColorV4>,
    pub max_width_px: Option<u32>,
    pub line_height_px: Option<f32>,
    pub text_align: TextAlignV4,
    pub layout: CaptionLayoutProfileV4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CaptionLayoutProfileV4 {
    LegacyCaptionV2,
    PinnedCaptionV4,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TextV4 {
    pub content: String,
    pub font: FontBindingV4,
    pub font_weight: FontWeightV4,
    pub font_style: FontStyleV4,
    pub decoration: TextDecorationV4,
    pub align: TextAlignV4,
    pub font_size: FontSizeV4,
    pub color: ColorV4,
    pub letter_spacing_px: f64,
    pub line_height_multiplier: f64,
    pub background: TextBackgroundV4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FontWeightV4 {
    Normal,
    Bold,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FontStyleV4 {
    Normal,
    Italic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TextDecorationV4 {
    None,
    Underline,
    LineThrough,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TextAlignV4 {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "unit", rename_all = "snake_case", deny_unknown_fields)]
pub enum FontSizeV4 {
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
pub struct TextBackgroundV4 {
    pub enabled: bool,
    pub color: ColorV4,
    /// Classic 0..100 radius percentage, not a pixel radius.
    pub corner_radius_percent: f64,
    /// Classic padding pixels at font-size 15; actual padding multiplies by
    /// font size / 15. Offsets remain pixels and do not use that multiplier.
    pub padding_x_at_size15: f64,
    pub padding_y_at_size15: f64,
    pub offset_x_px: f64,
    pub offset_y_px: f64,
}
