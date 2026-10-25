//! Ordered visual graphs and explicit Classic versus legacy geometry semantics.
use super::{CanvasSize, FontBinding, FontSize, FontStyle, FontWeight, TextDecoration};
use crate::Time;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Background {
    Color {
        color: Color,
    },
    /// Uses each sequence's main track, its visible media, source clock and
    /// retime; it does not blur the finished composition. Classic default 10.
    MainTrackBlur {
        intensity: f64,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VisualStyle {
    pub profile: VisualProfile,
    pub transform: Transform,
    pub opacity: f64,
    pub blend_mode: BlendMode,
    /// Ordered, identity-bearing stacks; disabled nodes remain editable.
    pub effects: Vec<SceneEffect>,
    pub masks: Vec<Mask>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VisualProfile {
    ClassicCanvasV31,
    LegacyCompositionV3,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Transform {
    /// Canvas pixels from center, positive right/down, for both media layouts.
    pub position_x_px: f64,
    pub position_y_px: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub rotation_degrees: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum BlendMode {
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
pub struct Color {
    pub encoding: ColorEncoding,
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    /// Straight alpha, independent of RGB encoding.
    pub alpha: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ColorEncoding {
    Srgb,
    LinearSrgb,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Graphic {
    pub geometry: GraphicGeometry,
    pub source_size: CanvasSize,
    pub fill: Color,
    pub stroke: Stroke,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GraphicGeometry {
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
pub struct Stroke {
    pub color: Color,
    pub width_px: f64,
    pub align: StrokeAlign,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StrokeAlign {
    Inside,
    Center,
    Outside,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SceneEffect {
    pub id: String,
    pub enabled: bool,
    pub effect: EffectKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EffectKind {
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
pub struct Mask {
    pub id: String,
    pub enabled: bool,
    pub shape: LayerMaskShape,
    pub inverted: bool,
    pub feather: MaskFeather,
    pub stroke: Stroke,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "unit", rename_all = "snake_case", deny_unknown_fields)]
pub enum MaskFeather {
    ClassicPixels { value: f64 },
    LegacyCanvasFraction { value: f64 },
}

/// Center offsets and dimensions are fractions of the untransformed element,
/// not normalized canvas coordinates. Classic scale is retained even where a
/// particular shape renderer does not use it, so recovery never loses fields.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoxMaskGeometry {
    pub center_x: f64,
    pub center_y: f64,
    pub width: f64,
    pub height: f64,
    pub rotation_degrees: f64,
    pub scale: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LayerMaskShape {
    Split {
        center_x: f64,
        center_y: f64,
        rotation_degrees: f64,
    },
    CinematicBars {
        geometry: BoxMaskGeometry,
    },
    Rectangle {
        geometry: BoxMaskGeometry,
    },
    Ellipse {
        geometry: BoxMaskGeometry,
    },
    Heart {
        geometry: BoxMaskGeometry,
    },
    Diamond {
        geometry: BoxMaskGeometry,
    },
    /// Classic mask star is fixed at five points with inner radius 0.45.
    Star {
        geometry: BoxMaskGeometry,
    },
    Text {
        mask: TextMask,
    },
    Freeform {
        path: FreeformMask,
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
pub struct TextMask {
    pub content: String,
    pub font: FontBinding,
    pub font_size: FontSize,
    pub font_weight: FontWeight,
    pub font_style: FontStyle,
    pub decoration: TextDecoration,
    pub letter_spacing_px: f64,
    pub line_height_multiplier: f64,
    pub center_x: f64,
    pub center_y: f64,
    pub rotation_degrees: f64,
    pub scale: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FreeformMask {
    pub points: Vec<FreeformPoint>,
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
pub struct FreeformPoint {
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
pub struct SceneTransition {
    pub id: String,
    pub outgoing_element_id: String,
    pub incoming_element_id: String,
    pub duration: Time,
    pub transition: TransitionKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TransitionKind {
    LegacyCrossfadeV3,
}
