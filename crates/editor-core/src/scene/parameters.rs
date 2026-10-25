//! Versioned V4 parameter semantics, independent of renderer capabilities.
//! UI hints never constrain imported keys. Stored keys remain raw; physical
//! projection is applied only after curve evaluation, without editing handles.
use super::*;

mod applicability;
mod catalog;
pub use applicability::{ParameterTarget, discrete_applies, scalar_applies};
pub use catalog::{
    COLOR_PARAMETERS, ColorMetadata, DISCRETE_PARAMETERS, DiscreteMetadata, SCALAR_PARAMETERS,
    ScalarMetadata, color_metadata, discrete_metadata, scalar_metadata,
};
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParameterUnit {
    Pixels,
    ScaleFactor,
    Degrees,
    Opacity,
    LinearAmplitude,
    Decibels,
    ClassicHeight90,
    LineHeightMultiplier,
    PaddingAtFontSize15Pixels,
    Percent,
    Count,
    ClassicBlurIntensity,
    ExposureEv,
    ColorAdjustmentFactor,
    ElementFraction,
    CanvasFraction,
    LinearSrgbComponent,
    StraightAlpha,
    StrokeAlignment,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiscreteInterpolation {
    Hold,
}

/// UI bounds/steps use displayed units: stored * display_multiplier. These are
/// authoring hints, never storage validation or post-curve rounding policy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiRange {
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: Option<f64>,
    pub display_multiplier: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StoredScalarRule {
    Finite,
    Positive,
    Nonnegative,
    Inclusive {
        min: f64,
        max: f64,
    },
    /// A finite dB value whose amplitude conversion must also remain finite.
    Decibels,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PhysicalScalarRule {
    Finite,
    /// No invented epsilon: a nonpositive layout input is an explicit error.
    Positive,
    Minimum {
        min: f64,
    },
    Clamp {
        min: f64,
        max: f64,
    },
    RoundClamp {
        min: f64,
        max: f64,
    },
}

/// Typed applicability includes storage-unit/profile matching; a mere target
/// kind does not imply that a Classic parameter can replace a legacy one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParameterApplicability {
    VisualElement,
    VisualElementOrTrack,
    LinearGain,
    DecibelGain,
    ClassicTextFontSize,
    PixelTextFontSize,
    Text,
    Graphic,
    GraphicRectangleOrPolygon,
    GraphicPolygon,
    GraphicStar,
    ClassicBlurEffect,
    LegacyBlurEffect,
    LegacyColorEffect,
    ClassicMaskGeometry,
    BoxMask,
    ScalableMask,
    ClassicMaskFeather,
    LegacyMaskFeather,
    Mask,
    ClassicMaskFontSize,
    PixelMaskFontSize,
    TextMask,
}

pub fn validate_stored_scalar(binding: &ScalarBinding, value: f64) -> Result<(), EditorError> {
    let rule = match binding {
        ScalarBinding::Property { parameter } => scalar_metadata(*parameter).stored,
        ScalarBinding::ColorComponent {
            parameter,
            component,
        } => {
            // Explicit metadata lookup also makes new color/component variants
            // subject to the registry rather than a wildcard scalar fallback.
            color_metadata(*parameter).component_unit(*component);
            StoredScalarRule::Inclusive { min: 0.0, max: 1.0 }
        }
    };
    match rule {
        StoredScalarRule::Finite => finite(value, "Stored scalar key"),
        StoredScalarRule::Positive => positive(value, "Stored scalar key"),
        StoredScalarRule::Nonnegative => nonnegative(value, "Stored scalar key"),
        StoredScalarRule::Inclusive { min, max } => bound(value, min, max, "Stored scalar key"),
        StoredScalarRule::Decibels => gain_amplitude(&Gain::Decibels { value }).map(|_| ()),
    }
}

/// Project a sampled raw value to a physical parameter in the SAME stored
/// unit. Decibels stay clamped dB; static audio lowering must use this same
/// projection before amplitude conversion, never raw gain_amplitude alone.
/// Opacity remains a raw layer coefficient, not a bounded encoded alpha;
/// blending and final pixel encoding are separate lowering responsibilities.
/// Nonfinite evaluation fails before clamping, including infinities that a
/// clamp could hide. Renderer-specific geometry/units are not handled here.
pub fn apply_sample(binding: &ScalarBinding, raw: f64) -> Result<f64, EditorError> {
    finite(raw, "Sampled scalar")?;
    let rule = match binding {
        ScalarBinding::Property { parameter } => scalar_metadata(*parameter).physical,
        ScalarBinding::ColorComponent {
            parameter,
            component,
        } => {
            color_metadata(*parameter).component_unit(*component);
            PhysicalScalarRule::Clamp { min: 0.0, max: 1.0 }
        }
    };
    let result = match rule {
        PhysicalScalarRule::Finite => raw,
        PhysicalScalarRule::Positive => {
            positive(raw, "Sampled positive layout parameter")?;
            raw
        }
        PhysicalScalarRule::Minimum { min } => raw.max(min),
        PhysicalScalarRule::Clamp { min, max } => raw.clamp(min, max),
        // Shape counts clamp into a positive range before round; this is
        // equivalent to Classic's positive Math.round followed by 3..12 clamp.
        PhysicalScalarRule::RoundClamp { min, max } => raw.clamp(min, max).round(),
    };
    finite(result, "Physical scalar")?;
    Ok(result)
}

pub fn validate_stored_discrete(
    parameter: DiscreteParameter,
    value: &DiscreteValue,
) -> Result<(), EditorError> {
    discrete_metadata(parameter);
    match (parameter, value) {
        (DiscreteParameter::GraphicStrokeAlign, DiscreteValue::StrokeAlign { value })
        | (DiscreteParameter::MaskStrokeAlign, DiscreteValue::StrokeAlign { value }) => match value
        {
            StrokeAlign::Inside | StrokeAlign::Center | StrokeAlign::Outside => Ok(()),
        },
    }
}
