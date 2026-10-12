//! Typed stable bindings, precise time/value tangents and explicit extrapolation.
use super::StrokeAlignV4;
use crate::Time;
use crate::project_v3::CubicBezierV1;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ChannelTargetV4 {
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
pub enum AnimationChannelV4 {
    Scalar {
        id: String,
        target: ChannelTargetV4,
        binding: ScalarBindingV4,
        keys: Vec<ScalarKeyV4>,
        extrapolation: ExtrapolationV4,
    },
    Discrete {
        id: String,
        target: ChannelTargetV4,
        parameter: DiscreteParameterV4,
        keys: Vec<DiscreteKeyV4>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ScalarBindingV4 {
    Property {
        parameter: ScalarParameterV4,
    },
    /// RGB values/tangents are linear-sRGB; alpha is not gamma encoded.
    ColorComponent {
        parameter: ColorParameterV4,
        component: ColorComponentV4,
    },
}

/// Each variant identifies both meaning and stored unit. Applicability and
/// post-evaluation bounds belong to the versioned parameter registry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ScalarParameterV4 {
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
pub enum ColorParameterV4 {
    TextColor,
    TextBackgroundColor,
    GraphicFill,
    GraphicStroke,
    MaskStroke,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ColorComponentV4 {
    Red,
    Green,
    Blue,
    Alpha,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DiscreteParameterV4 {
    GraphicStrokeAlign,
    MaskStrokeAlign,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DiscreteValueV4 {
    StrokeAlign { value: StrokeAlignV4 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScalarKeyV4 {
    pub id: String,
    /// Optional independent segment identity, preserving a V3 curve node ID.
    /// Classic segments are owned by the outgoing key and need no second ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub segment_id: Option<String>,
    /// Element-local time; track channels use sequence time.
    pub time: Time,
    pub value: f64,
    pub left_handle: Option<CurveHandleV4>,
    pub right_handle: Option<CurveHandleV4>,
    pub segment_to_next: ScalarSegmentV4,
    pub tangent_mode: TangentModeV4,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CurveHandleV4 {
    /// Signed exact time offset, not a normalized ease fraction.
    pub dt: Time,
    /// Signed value offset in the bound parameter's units.
    pub dv: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ScalarSegmentV4 {
    Step,
    Linear,
    /// Actual time/value control points; equal endpoints may still have a bump.
    /// A missing outgoing/incoming handle uses +/- segment duration / 3 and
    /// +/- value delta / 3, respectively. Both absent gives linear interpolation;
    /// tangent-mode hints never silently replace these stored/default controls.
    Bezier,
    /// Exact old ease interpolation, kept separate from time/value tangents.
    LegacyNormalizedBezier {
        curve: CubicBezierV1,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TangentModeV4 {
    Auto,
    Aligned,
    Broken,
    Flat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExtrapolationV4 {
    pub before: ExtrapolationModeV4,
    pub after: ExtrapolationModeV4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExtrapolationModeV4 {
    Hold,
    Linear,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DiscreteKeyV4 {
    pub id: String,
    pub time: Time,
    pub value: DiscreteValueV4,
}
