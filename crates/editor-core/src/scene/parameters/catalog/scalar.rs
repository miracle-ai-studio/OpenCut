//! Scalar declarations keep UI, storage and physical rules distinct.
use super::*;

fn ui(min: Option<f64>, max: Option<f64>, step: f64) -> Option<UiRange> {
    Some(UiRange {
        min,
        max,
        step: Some(step),
        display_multiplier: 1.0,
    })
}
fn percent_ui(min: f64, max: Option<f64>) -> Option<UiRange> {
    Some(UiRange {
        min: Some(min),
        max,
        step: Some(1.0),
        display_multiplier: 100.0,
    })
}

pub fn scalar_metadata(parameter: ScalarParameter) -> ScalarMetadata {
    use ParameterApplicability as A;
    use ParameterUnit as U;
    use PhysicalScalarRule as P;
    use ScalarParameter::*;
    use StoredScalarRule as S;
    let (unit, applies, ui, stored, physical) = match parameter {
        PositionXPx | PositionYPx => (
            U::Pixels,
            A::VisualElement,
            ui(Some(-100_000.0), None, 1.0),
            S::Finite,
            P::Finite,
        ),
        ScaleX | ScaleY => (
            U::ScaleFactor,
            A::VisualElement,
            ui(Some(0.01), None, 0.01),
            S::Finite,
            P::Finite,
        ),
        RotationDegrees => (
            U::Degrees,
            A::VisualElement,
            ui(Some(-360.0), Some(360.0), 1.0),
            S::Finite,
            P::Finite,
        ),
        Opacity => (
            U::Opacity,
            A::VisualElementOrTrack,
            ui(Some(0.0), Some(1.0), 0.01),
            S::Inclusive { min: 0.0, max: 1.0 },
            // Classic layer.wgsl multiplies alpha by this raw layer coefficient.
            // It is not encoded alpha: early clamp can also change blended RGB.
            P::Finite,
        ),
        GainLinearLegacy => (
            U::LinearAmplitude,
            A::LinearGain,
            None,
            S::Inclusive { min: 0.0, max: 4.0 },
            P::Clamp { min: 0.0, max: 4.0 },
        ),
        GainDecibels => (
            U::Decibels,
            A::DecibelGain,
            ui(Some(-60.0), Some(20.0), 0.01),
            S::Decibels,
            // timeline/audio-state.ts dBToLinear projects static volume and
            // automation alike before converting decibels to amplitude.
            P::Clamp {
                min: -60.0,
                max: 20.0,
            },
        ),
        FontSizeClassicHeight90 => (
            U::ClassicHeight90,
            A::ClassicTextFontSize,
            ui(Some(1.0), None, 1.0),
            S::Positive,
            P::Positive,
        ),
        FontSizeCanvasPixels => (
            U::Pixels,
            A::PixelTextFontSize,
            None,
            S::Positive,
            P::Positive,
        ),
        LetterSpacingPx => (
            U::Pixels,
            A::Text,
            ui(Some(-100.0), None, 0.1),
            S::Finite,
            P::Finite,
        ),
        LineHeightMultiplier => (
            U::LineHeightMultiplier,
            A::Text,
            ui(Some(0.1), None, 0.1),
            S::Positive,
            P::Positive,
        ),
        TextBackgroundPaddingXAtSize15 | TextBackgroundPaddingYAtSize15 => (
            U::PaddingAtFontSize15Pixels,
            A::Text,
            ui(Some(0.0), None, 1.0),
            S::Nonnegative,
            // text/layout.ts directly uses evaluated padding to construct the
            // rect; negative overshoot shrinks it. No runtime max(0) exists.
            P::Finite,
        ),
        TextBackgroundOffsetXPx | TextBackgroundOffsetYPx => (
            U::Pixels,
            A::Text,
            ui(Some(-100_000.0), None, 1.0),
            S::Finite,
            P::Finite,
        ),
        TextBackgroundCornerRadiusPercent => (
            U::Percent,
            A::Text,
            ui(Some(0.0), Some(100.0), 1.0),
            S::Inclusive {
                min: 0.0,
                max: 100.0,
            },
            P::Clamp {
                min: 0.0,
                max: 100.0,
            },
        ),
        GraphicStrokeWidthPx => (
            U::Pixels,
            A::Graphic,
            ui(Some(0.0), Some(64.0), 1.0),
            S::Nonnegative,
            P::Minimum { min: 0.0 },
        ),
        // Classic geometry saturates radius at 50%; text radius uses 100%.
        GraphicCornerRadiusPercent => (
            U::Percent,
            A::GraphicRectangleOrPolygon,
            ui(Some(0.0), Some(50.0), 1.0),
            S::Inclusive {
                min: 0.0,
                max: 100.0,
            },
            P::Clamp {
                min: 0.0,
                max: 50.0,
            },
        ),
        // Classic polygon/star render() clamps rounded counts, even though
        // fractional or larger imported keys must remain intact in storage.
        GraphicSides => (
            U::Count,
            A::GraphicPolygon,
            ui(Some(3.0), Some(12.0), 1.0),
            S::Inclusive {
                min: 3.0,
                max: f64::MAX,
            },
            P::RoundClamp {
                min: 3.0,
                max: 12.0,
            },
        ),
        GraphicPoints => (
            U::Count,
            A::GraphicStar,
            ui(Some(3.0), Some(12.0), 1.0),
            S::Inclusive {
                min: 2.0,
                max: f64::MAX,
            },
            P::RoundClamp {
                min: 3.0,
                max: 12.0,
            },
        ),
        GraphicDepthPercent => (
            U::Percent,
            A::GraphicStar,
            ui(Some(1.0), Some(99.0), 1.0),
            S::Inclusive {
                min: 0.0,
                max: 100.0,
            },
            P::Clamp {
                min: 1.0,
                max: 99.0,
            },
        ),
        ClassicBlurIntensity => (
            U::ClassicBlurIntensity,
            A::ClassicBlurEffect,
            ui(Some(0.0), Some(100.0), 1.0),
            S::Nonnegative,
            P::Minimum { min: 0.0 },
        ),
        LegacyBlurRadiusPx => (
            U::Pixels,
            A::LegacyBlurEffect,
            None,
            S::Inclusive {
                min: 0.0,
                max: 128.0,
            },
            P::Clamp {
                min: 0.0,
                max: 128.0,
            },
        ),
        LegacyExposureEv => (
            U::ExposureEv,
            A::LegacyColorEffect,
            None,
            S::Inclusive {
                min: -8.0,
                max: 8.0,
            },
            P::Clamp {
                min: -8.0,
                max: 8.0,
            },
        ),
        LegacyContrast | LegacySaturation => (
            U::ColorAdjustmentFactor,
            A::LegacyColorEffect,
            None,
            S::Inclusive { min: 0.0, max: 4.0 },
            P::Clamp { min: 0.0, max: 4.0 },
        ),
        MaskCenterX | MaskCenterY => (
            U::ElementFraction,
            A::ClassicMaskGeometry,
            percent_ui(-100.0, Some(100.0)),
            S::Finite,
            P::Finite,
        ),
        // masks/dimensions.ts defines the actual geometry floor as 0.01;
        // it is neither an arbitrary epsilon nor a UI import restriction.
        MaskWidth | MaskHeight => (
            U::ElementFraction,
            A::BoxMask,
            percent_ui(1.0, None),
            S::Positive,
            P::Minimum { min: 0.01 },
        ),
        MaskRotationDegrees => (
            U::Degrees,
            A::ClassicMaskGeometry,
            ui(Some(0.0), Some(360.0), 1.0),
            S::Finite,
            P::Finite,
        ),
        MaskScale => (
            U::ScaleFactor,
            A::ScalableMask,
            percent_ui(1.0, Some(500.0)),
            S::Nonnegative,
            P::Minimum { min: 0.0 },
        ),
        MaskFeatherClassicPixels => (
            U::Pixels,
            A::ClassicMaskFeather,
            ui(Some(0.0), Some(1000.0), 1.0),
            S::Nonnegative,
            P::Minimum { min: 0.0 },
        ),
        MaskFeatherLegacyCanvasFraction => (
            U::CanvasFraction,
            A::LegacyMaskFeather,
            None,
            S::Inclusive { min: 0.0, max: 0.5 },
            P::Clamp { min: 0.0, max: 0.5 },
        ),
        MaskStrokeWidthPx => (
            U::Pixels,
            A::Mask,
            ui(Some(0.0), Some(100.0), 1.0),
            S::Nonnegative,
            P::Minimum { min: 0.0 },
        ),
        MaskFontSizeClassicHeight90 => (
            U::ClassicHeight90,
            A::ClassicMaskFontSize,
            ui(Some(5.0), Some(300.0), 1.0),
            S::Positive,
            P::Positive,
        ),
        MaskFontSizeCanvasPixels => (
            U::Pixels,
            A::PixelMaskFontSize,
            None,
            S::Positive,
            P::Positive,
        ),
        MaskLetterSpacingPx => (U::Pixels, A::TextMask, None, S::Finite, P::Finite),
        MaskLineHeightMultiplier => (
            U::LineHeightMultiplier,
            A::TextMask,
            None,
            S::Positive,
            P::Positive,
        ),
    };
    ScalarMetadata {
        parameter,
        unit,
        applies,
        ui,
        stored,
        physical,
    }
}
