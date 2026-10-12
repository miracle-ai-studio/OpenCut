//! Ordered visual graphs and explicit Classic versus legacy geometry semantics.
use super::{CanvasSizeV4, FontBindingV4, FontSizeV4, FontStyleV4, FontWeightV4, TextDecorationV4};
use crate::Time;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BackgroundV4 {
    Color {
        color: ColorV4,
    },
    /// Uses each sequence's main track, its visible media, source clock and
    /// retime; it does not blur the finished composition. Classic default 10.
    MainTrackBlur {
        intensity: f64,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VisualStyleV4 {
    pub profile: VisualProfileV4,
    pub transform: TransformV4,
    pub opacity: f64,
    pub blend_mode: BlendModeV4,
    /// Ordered, identity-bearing stacks; disabled nodes remain editable.
    pub effects: Vec<EffectV4>,
    pub masks: Vec<MaskV4>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VisualProfileV4 {
    ClassicCanvasV31,
    LegacyCompositionV3,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransformV4 {
    /// Canvas pixels from center, positive right/down.
    pub position_x_px: f64,
    pub position_y_px: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub rotation_degrees: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum BlendModeV4 {
    Normal,
    Darken,
    Multiply,
    ColorBurn,
    Lighten,
    Screen,
    PlusLighter,
    ColorDodge,
    Overlay,
    SoftLight,
    HardLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ColorV4 {
    pub encoding: ColorEncodingV4,
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    /// Straight alpha, independent of RGB encoding.
    pub alpha: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ColorEncodingV4 {
    Srgb,
    LinearSrgb,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphicV4 {
    pub geometry: GraphicGeometryV4,
    pub source_size: CanvasSizeV4,
    pub fill: ColorV4,
    pub stroke: StrokeV4,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GraphicGeometryV4 {
    Rectangle {
        corner_radius_percent: f64,
    },
    Ellipse,
    /// Fractional animated counts are preserved; Classic rounds at evaluation.
    Polygon {
        sides: f64,
        corner_radius_percent: f64,
    },
    Star {
        points: f64,
        depth_percent: f64,
    },
    LegacyRectangle {
        width_px: f64,
        height_px: f64,
    },
    LegacyEllipse {
        radius_x_px: f64,
        radius_y_px: f64,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StrokeV4 {
    pub color: ColorV4,
    pub width_px: f64,
    pub align: StrokeAlignV4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StrokeAlignV4 {
    Inside,
    Center,
    Outside,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EffectV4 {
    pub id: String,
    pub enabled: bool,
    pub effect: EffectKindV4,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EffectKindV4 {
    /// sigmaX=intensity/5*width/1920, sigmaY=intensity/5*height/1080.
    ClassicBlur {
        intensity: f64,
    },
    LegacyGaussianBlur {
        radius_px: f64,
    },
    /// Preserves the existing V3 implementation, including its exposure naming;
    /// this is not a new claim of physically correct EV color processing.
    LegacyColorAdjust {
        exposure_ev: f64,
        contrast: f64,
        saturation: f64,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MaskV4 {
    pub id: String,
    pub enabled: bool,
    pub shape: MaskShapeV4,
    pub inverted: bool,
    pub feather: MaskFeatherV4,
    pub stroke: StrokeV4,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "unit", rename_all = "snake_case", deny_unknown_fields)]
pub enum MaskFeatherV4 {
    ClassicPixels { value: f64 },
    LegacyCanvasFraction { value: f64 },
}

/// Center offsets and dimensions are fractions of the untransformed element,
/// not normalized canvas coordinates. Classic scale is retained even where a
/// particular shape renderer does not use it, so recovery never loses fields.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoxMaskGeometryV4 {
    pub center_x: f64,
    pub center_y: f64,
    pub width: f64,
    pub height: f64,
    pub rotation_degrees: f64,
    pub scale: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MaskShapeV4 {
    Split {
        center_x: f64,
        center_y: f64,
        rotation_degrees: f64,
    },
    CinematicBars {
        geometry: BoxMaskGeometryV4,
    },
    Rectangle {
        geometry: BoxMaskGeometryV4,
    },
    Ellipse {
        geometry: BoxMaskGeometryV4,
    },
    Heart {
        geometry: BoxMaskGeometryV4,
    },
    Diamond {
        geometry: BoxMaskGeometryV4,
    },
    /// Classic mask star is fixed at five points with inner radius 0.45.
    Star {
        geometry: BoxMaskGeometryV4,
    },
    Text {
        mask: TextMaskV4,
    },
    Freeform {
        path: FreeformMaskV4,
    },
    /// V3 masks use top-left canvas coordinates and canvas-normalized sizes.
    LegacyRectangle {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    },
    LegacyEllipse {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TextMaskV4 {
    pub content: String,
    pub font: FontBindingV4,
    pub font_size: FontSizeV4,
    pub font_weight: FontWeightV4,
    pub font_style: FontStyleV4,
    pub decoration: TextDecorationV4,
    pub letter_spacing_px: f64,
    pub line_height_multiplier: f64,
    pub center_x: f64,
    pub center_y: f64,
    pub rotation_degrees: f64,
    pub scale: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FreeformMaskV4 {
    pub points: Vec<FreeformPointV4>,
    pub closed: bool,
    pub center_x: f64,
    pub center_y: f64,
    pub rotation_degrees: f64,
    pub scale: f64,
}

/// Coordinates and absolute handle positions are normalized element-local
/// coordinates around its center, matching Classic's stored path geometry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FreeformPointV4 {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub in_x: f64,
    pub in_y: f64,
    pub out_x: f64,
    pub out_y: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransitionV4 {
    pub id: String,
    pub outgoing_element_id: String,
    pub incoming_element_id: String,
    pub duration: Time,
    pub transition: TransitionKindV4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TransitionKindV4 {
    LegacyCrossfadeV3,
}
