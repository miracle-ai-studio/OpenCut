use super::*;

fn property(parameter: ScalarParameter) -> ScalarBinding {
    ScalarBinding::Property { parameter }
}

#[test]
fn catalog_covers_every_declared_variant_once() {
    // Exhaustive matches in the catalog enforce metadata coverage at compile
    // time; discovery must also enumerate each current variant exactly once.
    fn distinct(values: impl Iterator<Item = String>, expected: usize) {
        let values: Vec<_> = values.collect();
        assert_eq!(values.len(), expected);
        assert_eq!(values.iter().collect::<HashSet<_>>().len(), expected);
    }
    distinct(
        SCALAR_PARAMETERS
            .iter()
            .map(|v| serde_json::to_string(v).unwrap()),
        40,
    );
    distinct(
        COLOR_PARAMETERS
            .iter()
            .map(|v| serde_json::to_string(v).unwrap()),
        5,
    );
    distinct(
        DISCRETE_PARAMETERS
            .iter()
            .map(|v| serde_json::to_string(v).unwrap()),
        2,
    );
    for parameter in SCALAR_PARAMETERS {
        assert_eq!(scalar_metadata(*parameter).parameter, *parameter);
    }
    for parameter in COLOR_PARAMETERS {
        let metadata = color_metadata(*parameter);
        assert_eq!(metadata.parameter, *parameter);
        for component in [
            ColorComponent::Red,
            ColorComponent::Green,
            ColorComponent::Blue,
        ] {
            assert_eq!(
                metadata.component_unit(component),
                ParameterUnit::LinearSrgbComponent
            );
        }
        assert_eq!(
            metadata.component_unit(ColorComponent::Alpha),
            ParameterUnit::StraightAlpha
        );
    }
    for parameter in DISCRETE_PARAMETERS {
        assert_eq!(discrete_metadata(*parameter).parameter, *parameter);
        assert_eq!(
            discrete_metadata(*parameter).interpolation,
            DiscreteInterpolation::Hold
        );
    }
}

#[test]
fn ui_caps_do_not_reject_imported_keys_or_rewrite_raw_samples() {
    use ScalarParameter::*;
    for (parameter, imported, physical) in [
        (GraphicSides, 24.5, 12.0),
        (GraphicPoints, 2.5, 3.0),
        (GraphicCornerRadiusPercent, 75.0, 50.0),
        (GraphicDepthPercent, 100.0, 99.0),
        (ClassicBlurIntensity, 400.0, 400.0),
        (MaskFeatherClassicPixels, 1200.0, 1200.0),
        (MaskFontSizeClassicHeight90, 400.0, 400.0),
        (GainDecibels, 30.0, 20.0),
    ] {
        let binding = property(parameter);
        validate_stored_scalar(&binding, imported).unwrap();
        assert_eq!(apply_sample(&binding, imported).unwrap(), physical);
    }
    assert_eq!(
        scalar_metadata(MaskWidth).ui.unwrap().display_multiplier,
        100.0
    );
    assert_eq!(apply_sample(&property(GraphicSides), 3.5).unwrap(), 4.0);
    assert_eq!(apply_sample(&property(GraphicPoints), 11.5).unwrap(), 12.0);
}

#[test]
fn overshoot_is_projected_after_evaluation_with_no_layout_epsilon() {
    use ScalarParameter::*;
    // Authored keys obey their stored domain; interpolated coefficients and
    // padding may legitimately cross that domain in Classic's runtime.
    assert!(validate_stored_scalar(&property(Opacity), 1.75).is_err());
    assert!(validate_stored_scalar(&property(TextBackgroundPaddingXAtSize15), -3.0).is_err());
    for (parameter, raw, expected) in [
        (Opacity, 1.75, 1.75),
        (Opacity, -0.2, -0.2),
        (TextBackgroundPaddingXAtSize15, -3.0, -3.0),
        (TextBackgroundPaddingYAtSize15, -4.0, -4.0),
        (GraphicStrokeWidthPx, -3.0, 0.0),
        (ClassicBlurIntensity, -2.0, 0.0),
        (MaskWidth, -1.0, 0.01),
        (MaskHeight, 0.001, 0.01),
        (MaskScale, -2.0, 0.0),
        (GraphicSides, -20.0, 3.0),
        (GraphicDepthPercent, -10.0, 1.0),
        (LegacyExposureEv, 20.0, 8.0),
        (LegacySaturation, -1.0, 0.0),
        (MaskFeatherLegacyCanvasFraction, 2.0, 0.5),
        (ScaleX, -2.0, -2.0),
        (RotationDegrees, 1080.0, 1080.0),
    ] {
        assert_eq!(apply_sample(&property(parameter), raw).unwrap(), expected);
    }
    for parameter in [
        FontSizeClassicHeight90,
        FontSizeCanvasPixels,
        LineHeightMultiplier,
        MaskFontSizeClassicHeight90,
        MaskFontSizeCanvasPixels,
        MaskLineHeightMultiplier,
    ] {
        assert!(apply_sample(&property(parameter), 0.0).is_err());
        assert!(apply_sample(&property(parameter), -0.5).is_err());
        assert_eq!(
            apply_sample(&property(parameter), f64::MIN_POSITIVE).unwrap(),
            f64::MIN_POSITIVE
        );
    }
    for parameter in SCALAR_PARAMETERS {
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(apply_sample(&property(*parameter), value).is_err());
            assert!(validate_stored_scalar(&property(*parameter), value).is_err());
        }
    }
    assert_eq!(
        apply_sample(&property(GainDecibels), f64::MAX).unwrap(),
        20.0
    );
    let color = ScalarBinding::ColorComponent {
        parameter: ColorParameter::MaskStroke,
        component: ColorComponent::Alpha,
    };
    assert!(validate_stored_scalar(&color, 1.2).is_err());
    assert_eq!(apply_sample(&color, 1.2).unwrap(), 1.0);
    assert!(apply_sample(&color, f64::INFINITY).is_err());
}

#[test]
fn classic_audio_projects_raw_db_before_amplitude_conversion() {
    let binding = property(ScalarParameter::GainDecibels);
    for (raw, expected_db, expected_amplitude) in [
        (20.0, 20.0, 10.0),
        (30.0, 20.0, 10.0),
        (-60.0, -60.0, 0.001),
        (-80.0, -60.0, 0.001),
    ] {
        validate_stored_scalar(&binding, raw).unwrap();
        let physical = apply_sample(&binding, raw).unwrap();
        assert_eq!(physical, expected_db);
        assert_eq!(
            gain_amplitude(&Gain::Decibels { value: physical }).unwrap(),
            expected_amplitude
        );
    }
    for nonfinite in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(apply_sample(&binding, nonfinite).is_err());
    }
}

fn track(kind: SceneTrackKind, gain: Gain) -> SceneTrack {
    SceneTrack {
        id: "track".into(),
        name: "Track".into(),
        kind,
        enabled: true,
        muted: false,
        locked: false,
        opacity: 1.0,
        gain,
        elements: vec![],
        channels: vec![],
    }
}
fn mask(feather: MaskFeather, shape: LayerMaskShape) -> Mask {
    Mask {
        id: "mask".into(),
        enabled: true,
        shape,
        inverted: false,
        feather,
        stroke: Stroke {
            color: Color {
                encoding: ColorEncoding::Srgb,
                red: 1.0,
                green: 1.0,
                blue: 1.0,
                alpha: 1.0,
            },
            width_px: 0.0,
            align: StrokeAlign::Center,
        },
    }
}

#[test]
fn registry_enforces_target_profile_and_units() {
    let audio = track(SceneTrackKind::Audio, Gain::Decibels { value: 0.0 });
    let video = track(SceneTrackKind::Video, Gain::LinearLegacy { value: 1.0 });
    let classic = SceneEffect {
        id: "effect".into(),
        enabled: true,
        effect: EffectKind::ClassicBlur { intensity: 15.0 },
    };
    let legacy = SceneEffect {
        id: "legacy".into(),
        enabled: true,
        effect: EffectKind::LegacyGaussianBlur { radius_px: 4.0 },
    };
    assert!(scalar_applies(
        &property(ScalarParameter::GainDecibels),
        &ParameterTarget::Track(&audio)
    ));
    assert!(!scalar_applies(
        &property(ScalarParameter::GainLinearLegacy),
        &ParameterTarget::Track(&audio)
    ));
    assert!(!scalar_applies(
        &property(ScalarParameter::Opacity),
        &ParameterTarget::Track(&audio)
    ));
    assert!(scalar_applies(
        &property(ScalarParameter::Opacity),
        &ParameterTarget::Track(&video)
    ));
    assert!(scalar_applies(
        &property(ScalarParameter::ClassicBlurIntensity),
        &ParameterTarget::Effect(&classic)
    ));
    assert!(!scalar_applies(
        &property(ScalarParameter::ClassicBlurIntensity),
        &ParameterTarget::Effect(&legacy)
    ));
    let split = mask(
        MaskFeather::ClassicPixels { value: 2.0 },
        LayerMaskShape::Split {
            center_x: 0.0,
            center_y: 0.0,
            rotation_degrees: 0.0,
        },
    );
    assert!(scalar_applies(
        &property(ScalarParameter::MaskCenterX),
        &ParameterTarget::Mask(&split)
    ));
    assert!(!scalar_applies(
        &property(ScalarParameter::MaskWidth),
        &ParameterTarget::Mask(&split)
    ));
    assert!(!scalar_applies(
        &property(ScalarParameter::MaskFeatherLegacyCanvasFraction),
        &ParameterTarget::Mask(&split)
    ));
    assert!(discrete_applies(
        DiscreteParameter::MaskStrokeAlign,
        &ParameterTarget::Mask(&split)
    ));
    assert!(!discrete_applies(
        DiscreteParameter::GraphicStrokeAlign,
        &ParameterTarget::Mask(&split)
    ));
    for parameter in DISCRETE_PARAMETERS {
        for value in [
            StrokeAlign::Inside,
            StrokeAlign::Center,
            StrokeAlign::Outside,
        ] {
            validate_stored_discrete(*parameter, &DiscreteValue::StrokeAlign { value }).unwrap();
        }
    }
}
