//! Exhaustive declarations: new public parameter variants require a rule here.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScalarMetadataV4 {
    pub parameter: ScalarParameterV4,
    pub unit: ParameterUnitV4,
    pub applies: ParameterApplicabilityV4,
    pub ui: Option<UiRangeV4>,
    pub stored: StoredScalarRuleV4,
    pub physical: PhysicalScalarRuleV4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColorMetadataV4 {
    pub parameter: ColorParameterV4,
    pub applies: ParameterApplicabilityV4,
}
impl ColorMetadataV4 {
    pub fn component_unit(self, component: ColorComponentV4) -> ParameterUnitV4 {
        match component {
            ColorComponentV4::Red | ColorComponentV4::Green | ColorComponentV4::Blue => {
                ParameterUnitV4::LinearSrgbComponent
            }
            ColorComponentV4::Alpha => ParameterUnitV4::StraightAlpha,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiscreteMetadataV4 {
    pub parameter: DiscreteParameterV4,
    pub unit: ParameterUnitV4,
    pub applies: ParameterApplicabilityV4,
    /// There is no numeric interpolation or curve for stroke alignment.
    pub interpolation: DiscreteInterpolationV4,
}

mod scalar;
pub use scalar::scalar_metadata;

pub fn color_metadata(parameter: ColorParameterV4) -> ColorMetadataV4 {
    let applies = match parameter {
        ColorParameterV4::TextColor | ColorParameterV4::TextBackgroundColor => {
            ParameterApplicabilityV4::Text
        }
        ColorParameterV4::GraphicFill | ColorParameterV4::GraphicStroke => {
            ParameterApplicabilityV4::Graphic
        }
        ColorParameterV4::MaskStroke => ParameterApplicabilityV4::Mask,
    };
    ColorMetadataV4 { parameter, applies }
}

pub fn discrete_metadata(parameter: DiscreteParameterV4) -> DiscreteMetadataV4 {
    let applies = match parameter {
        DiscreteParameterV4::GraphicStrokeAlign => ParameterApplicabilityV4::Graphic,
        DiscreteParameterV4::MaskStrokeAlign => ParameterApplicabilityV4::Mask,
    };
    DiscreteMetadataV4 {
        parameter,
        unit: ParameterUnitV4::StrokeAlignment,
        applies,
        interpolation: DiscreteInterpolationV4::Hold,
    }
}

pub const SCALAR_PARAMETERS_V4: &[ScalarParameterV4] = &[
    ScalarParameterV4::PositionXPx,
    ScalarParameterV4::PositionYPx,
    ScalarParameterV4::ScaleX,
    ScalarParameterV4::ScaleY,
    ScalarParameterV4::RotationDegrees,
    ScalarParameterV4::Opacity,
    ScalarParameterV4::GainLinearLegacy,
    ScalarParameterV4::GainDecibels,
    ScalarParameterV4::FontSizeClassicHeight90,
    ScalarParameterV4::FontSizeCanvasPixels,
    ScalarParameterV4::LetterSpacingPx,
    ScalarParameterV4::LineHeightMultiplier,
    ScalarParameterV4::TextBackgroundPaddingXAtSize15,
    ScalarParameterV4::TextBackgroundPaddingYAtSize15,
    ScalarParameterV4::TextBackgroundOffsetXPx,
    ScalarParameterV4::TextBackgroundOffsetYPx,
    ScalarParameterV4::TextBackgroundCornerRadiusPercent,
    ScalarParameterV4::GraphicStrokeWidthPx,
    ScalarParameterV4::GraphicCornerRadiusPercent,
    ScalarParameterV4::GraphicSides,
    ScalarParameterV4::GraphicPoints,
    ScalarParameterV4::GraphicDepthPercent,
    ScalarParameterV4::ClassicBlurIntensity,
    ScalarParameterV4::LegacyBlurRadiusPx,
    ScalarParameterV4::LegacyExposureEv,
    ScalarParameterV4::LegacyContrast,
    ScalarParameterV4::LegacySaturation,
    ScalarParameterV4::MaskCenterX,
    ScalarParameterV4::MaskCenterY,
    ScalarParameterV4::MaskWidth,
    ScalarParameterV4::MaskHeight,
    ScalarParameterV4::MaskRotationDegrees,
    ScalarParameterV4::MaskScale,
    ScalarParameterV4::MaskFeatherClassicPixels,
    ScalarParameterV4::MaskFeatherLegacyCanvasFraction,
    ScalarParameterV4::MaskStrokeWidthPx,
    ScalarParameterV4::MaskFontSizeClassicHeight90,
    ScalarParameterV4::MaskFontSizeCanvasPixels,
    ScalarParameterV4::MaskLetterSpacingPx,
    ScalarParameterV4::MaskLineHeightMultiplier,
];
pub const COLOR_PARAMETERS_V4: &[ColorParameterV4] = &[
    ColorParameterV4::TextColor,
    ColorParameterV4::TextBackgroundColor,
    ColorParameterV4::GraphicFill,
    ColorParameterV4::GraphicStroke,
    ColorParameterV4::MaskStroke,
];
pub const DISCRETE_PARAMETERS_V4: &[DiscreteParameterV4] = &[
    DiscreteParameterV4::GraphicStrokeAlign,
    DiscreteParameterV4::MaskStrokeAlign,
];
