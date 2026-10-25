//! Typed stable bindings, precise time/value tangents and explicit extrapolation.
use super::StrokeAlign;
use crate::Time;
use crate::historical_visual::HistoricalVisualCubicBezier;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ChannelTarget {
    Element {
        element_id: String,
    },
    Effect {
        element_id: String,
        effect_id: String,
    },
    Mask {
        element_id: String,
        mask_id: String,
    },
    Track {
        track_id: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AnimationChannel {
    Scalar {
        id: String,
        target: ChannelTarget,
        binding: ScalarBinding,
        keys: Vec<ScalarKey>,
        extrapolation: Extrapolation,
    },
    Discrete {
        id: String,
        target: ChannelTarget,
        parameter: DiscreteParameter,
        keys: Vec<DiscreteKey>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ScalarBinding {
    Property {
        parameter: ScalarParameter,
    },
    /// RGB values/tangents are linear-sRGB; alpha is not gamma encoded.
    ColorComponent {
        parameter: ColorParameter,
        component: ColorComponent,
    },
}

/// Each variant identifies both meaning and stored unit. Applicability and
/// post-evaluation bounds belong to the versioned parameter registry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ScalarParameter {
    PositionXPx,
    PositionYPx,
    ScaleX,
    ScaleY,
    RotationDegrees,
    Opacity,
    GainLinearLegacy,
    GainDecibels,
    FontSizeClassicHeight90,
    FontSizeCanvasPixels,
    LetterSpacingPx,
    LineHeightMultiplier,
    TextBackgroundPaddingXAtSize15,
    TextBackgroundPaddingYAtSize15,
    TextBackgroundOffsetXPx,
    TextBackgroundOffsetYPx,
    TextBackgroundCornerRadiusPercent,
    GraphicStrokeWidthPx,
    GraphicCornerRadiusPercent,
    GraphicSides,
    GraphicPoints,
    GraphicDepthPercent,
    ClassicBlurIntensity,
    LegacyBlurRadiusPx,
    LegacyExposureEv,
    LegacyContrast,
    LegacySaturation,
    MaskCenterX,
    MaskCenterY,
    MaskWidth,
    MaskHeight,
    MaskRotationDegrees,
    MaskScale,
    MaskFeatherClassicPixels,
    MaskFeatherLegacyCanvasFraction,
    MaskStrokeWidthPx,
    MaskFontSizeClassicHeight90,
    MaskFontSizeCanvasPixels,
    MaskLetterSpacingPx,
    MaskLineHeightMultiplier,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ColorParameter {
    TextColor,
    TextBackgroundColor,
    GraphicFill,
    GraphicStroke,
    MaskStroke,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ColorComponent {
    Red,
    Green,
    Blue,
    Alpha,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DiscreteParameter {
    GraphicStrokeAlign,
    MaskStrokeAlign,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DiscreteValue {
    StrokeAlign { value: StrokeAlign },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScalarKey {
    pub id: String,
    /// Optional independent segment identity, preserving a V3 curve node ID.
    /// Classic segments are owned by the outgoing key and need no second ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub segment_id: Option<String>,
    /// Element-local time; track channels use sequence time.
    pub time: Time,
    pub value: f64,
    pub left_handle: Option<CurveHandle>,
    pub right_handle: Option<CurveHandle>,
    pub segment_to_next: ScalarSegment,
    pub tangent_mode: TangentMode,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CurveHandle {
    /// Signed exact time offset, not a normalized ease fraction.
    pub dt: Time,
    /// Signed value offset in the bound parameter's units.
    pub dv: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ScalarSegment {
    Step,
    Linear,
    /// Actual time/value control points; equal endpoints may still have a bump.
    /// A missing outgoing/incoming handle uses +/- segment duration / 3 and
    /// +/- value delta / 3, respectively. Both absent gives linear interpolation;
    /// tangent-mode hints never silently replace these stored/default controls.
    Bezier,
    /// Exact old ease interpolation, kept separate from time/value tangents.
    LegacyNormalizedBezier {
        curve: HistoricalVisualCubicBezier,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TangentMode {
    Auto,
    Aligned,
    Broken,
    Flat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Extrapolation {
    pub before: ExtrapolationMode,
    pub after: ExtrapolationMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExtrapolationMode {
    Hold,
    Linear,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DiscreteKey {
    pub id: String,
    pub time: Time,
    pub value: DiscreteValue,
}
