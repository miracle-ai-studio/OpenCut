//! Exhaustive declarations: new public parameter variants require a rule here.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScalarMetadata {
    pub parameter: ScalarParameter,
    pub unit: ParameterUnit,
    pub applies: ParameterApplicability,
    pub ui: Option<UiRange>,
    pub stored: StoredScalarRule,
    pub physical: PhysicalScalarRule,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColorMetadata {
    pub parameter: ColorParameter,
    pub applies: ParameterApplicability,
}
impl ColorMetadata {
    pub fn component_unit(self, component: ColorComponent) -> ParameterUnit {
        match component {
            ColorComponent::Red | ColorComponent::Green | ColorComponent::Blue => {
                ParameterUnit::LinearSrgbComponent
            }
            ColorComponent::Alpha => ParameterUnit::StraightAlpha,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiscreteMetadata {
    pub parameter: DiscreteParameter,
    pub unit: ParameterUnit,
    pub applies: ParameterApplicability,
    /// There is no numeric interpolation or curve for stroke alignment.
    pub interpolation: DiscreteInterpolation,
}

mod scalar;
pub use scalar::scalar_metadata;

pub fn color_metadata(parameter: ColorParameter) -> ColorMetadata {
    let applies = match parameter {
        ColorParameter::TextColor | ColorParameter::TextBackgroundColor => {
            ParameterApplicability::Text
        }
        ColorParameter::GraphicFill | ColorParameter::GraphicStroke => {
            ParameterApplicability::Graphic
        }
        ColorParameter::MaskStroke => ParameterApplicability::Mask,
    };
    ColorMetadata { parameter, applies }
}

pub fn discrete_metadata(parameter: DiscreteParameter) -> DiscreteMetadata {
    let applies = match parameter {
        DiscreteParameter::GraphicStrokeAlign => ParameterApplicability::Graphic,
        DiscreteParameter::MaskStrokeAlign => ParameterApplicability::Mask,
    };
    DiscreteMetadata {
        parameter,
        unit: ParameterUnit::StrokeAlignment,
        applies,
        interpolation: DiscreteInterpolation::Hold,
    }
}

pub const SCALAR_PARAMETERS: &[ScalarParameter] = &[
    ScalarParameter::PositionXPx,
    ScalarParameter::PositionYPx,
    ScalarParameter::ScaleX,
    ScalarParameter::ScaleY,
    ScalarParameter::RotationDegrees,
    ScalarParameter::Opacity,
    ScalarParameter::GainLinearLegacy,
    ScalarParameter::GainDecibels,
    ScalarParameter::FontSizeClassicHeight90,
    ScalarParameter::FontSizeCanvasPixels,
    ScalarParameter::LetterSpacingPx,
    ScalarParameter::LineHeightMultiplier,
    ScalarParameter::TextBackgroundPaddingXAtSize15,
    ScalarParameter::TextBackgroundPaddingYAtSize15,
    ScalarParameter::TextBackgroundOffsetXPx,
    ScalarParameter::TextBackgroundOffsetYPx,
    ScalarParameter::TextBackgroundCornerRadiusPercent,
    ScalarParameter::GraphicStrokeWidthPx,
    ScalarParameter::GraphicCornerRadiusPercent,
    ScalarParameter::GraphicSides,
    ScalarParameter::GraphicPoints,
    ScalarParameter::GraphicDepthPercent,
    ScalarParameter::ClassicBlurIntensity,
    ScalarParameter::LegacyBlurRadiusPx,
    ScalarParameter::LegacyExposureEv,
    ScalarParameter::LegacyContrast,
    ScalarParameter::LegacySaturation,
    ScalarParameter::MaskCenterX,
    ScalarParameter::MaskCenterY,
    ScalarParameter::MaskWidth,
    ScalarParameter::MaskHeight,
    ScalarParameter::MaskRotationDegrees,
    ScalarParameter::MaskScale,
    ScalarParameter::MaskFeatherClassicPixels,
    ScalarParameter::MaskFeatherLegacyCanvasFraction,
    ScalarParameter::MaskStrokeWidthPx,
    ScalarParameter::MaskFontSizeClassicHeight90,
    ScalarParameter::MaskFontSizeCanvasPixels,
    ScalarParameter::MaskLetterSpacingPx,
    ScalarParameter::MaskLineHeightMultiplier,
];
pub const COLOR_PARAMETERS: &[ColorParameter] = &[
    ColorParameter::TextColor,
    ColorParameter::TextBackgroundColor,
    ColorParameter::GraphicFill,
    ColorParameter::GraphicStroke,
    ColorParameter::MaskStroke,
];
pub const DISCRETE_PARAMETERS: &[DiscreteParameter] = &[
    DiscreteParameter::GraphicStrokeAlign,
    DiscreteParameter::MaskStrokeAlign,
];
