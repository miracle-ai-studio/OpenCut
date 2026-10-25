use super::*;
use opencut_contracts::historical_visual::HistoricalVisualCubicBezier;

fn at(ticks: i64, timebase: u32) -> Time {
    Time {
        ticks: ticks.to_string(),
        timebase,
    }
}
fn key(id: &str, time: Time, value: f64) -> ScalarKey {
    ScalarKey {
        id: id.into(),
        segment_id: None,
        time,
        value,
        left_handle: None,
        right_handle: None,
        segment_to_next: ScalarSegment::Bezier,
        tangent_mode: TangentMode::Flat,
    }
}
fn scalar(parameter: ScalarParameter, keys: Vec<ScalarKey>) -> AnimationChannel {
    AnimationChannel::Scalar {
        id: "channel".into(),
        target: ChannelTarget::Element {
            element_id: "element".into(),
        },
        binding: ScalarBinding::Property { parameter },
        keys,
        extrapolation: Extrapolation {
            before: ExtrapolationMode::Linear,
            after: ExtrapolationMode::Linear,
        },
    }
}
fn sample(channel: &AnimationChannel, time: Time) -> (f64, f64) {
    let SampledChannel::Scalar { raw, physical, .. } =
        sample_animation_channel_scene(channel, &time).unwrap()
    else {
        panic!("scalar expected")
    };
    (raw, physical)
}
fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-11,
        "expected {expected}, got {actual}"
    );
}
fn keys_mut(channel: &mut AnimationChannel) -> &mut Vec<ScalarKey> {
    let AnimationChannel::Scalar { keys, .. } = channel else {
        panic!("scalar expected")
    };
    keys
}
fn rejected(channel: &AnimationChannel, message: &str) {
    // Validate the whole curve even at the first key, rather than skipping bad
    // controls in a segment that this particular sample does not visit.
    let error = sample_animation_channel_scene(channel, &at(0, 1)).unwrap_err();
    assert!(
        error.message.contains(message),
        "unexpected: {}",
        error.message
    );
}

#[test]
fn validated_sampler_reuses_exact_key_times_and_rejects_invalid_inputs() {
    let mut channel = scalar(
        ScalarParameter::GainLinearLegacy,
        vec![key("start", at(0, 1), 0.0), key("end", at(2, 1), 1.0)],
    );
    keys_mut(&mut channel)[0].segment_to_next = ScalarSegment::Linear;
    {
        let sampler = ValidatedAnimationChannel::new(&channel).unwrap();
        for n in 0..=8 {
            let SampledChannel::Scalar { raw, physical, .. } = sampler.sample(&at(n, 4)).unwrap()
            else {
                panic!("scalar")
            };
            assert_eq!(raw, n as f64 / 8.0);
            assert_eq!(physical, raw);
        }
        assert!(
            sampler
                .sample(&Time {
                    ticks: "invalid".into(),
                    timebase: 1
                })
                .is_err()
        );
    }
    // The immutable proof must be dropped before authored keys can change.
    keys_mut(&mut channel)[1].id = "start".into();
    assert!(ValidatedAnimationChannel::new(&channel).is_err());
}

#[test]
fn step_linear_and_discrete_use_exact_key_boundary_ownership() {
    let mut channel = scalar(
        ScalarParameter::PositionXPx,
        vec![key("a", at(1, 3), 2.0), key("b", at(2, 3), 6.0)],
    );
    keys_mut(&mut channel)[0].segment_to_next = ScalarSegment::Step;
    assert_eq!(sample(&channel, at(1, 2)), (2.0, 2.0));
    assert_eq!(sample(&channel, at(2000, 3000)), (6.0, 6.0));
    keys_mut(&mut channel)[0].segment_to_next = ScalarSegment::Linear;
    assert_eq!(sample(&channel, at(1, 2)), (4.0, 4.0));
    let discrete = AnimationChannel::Discrete {
        id: "stroke".into(),
        target: ChannelTarget::Element {
            element_id: "shape".into(),
        },
        parameter: DiscreteParameter::GraphicStrokeAlign,
        keys: vec![
            DiscreteKey {
                id: "d0".into(),
                time: at(1, 3),
                value: DiscreteValue::StrokeAlign {
                    value: StrokeAlign::Inside,
                },
            },
            DiscreteKey {
                id: "d1".into(),
                time: at(2, 3),
                value: DiscreteValue::StrokeAlign {
                    value: StrokeAlign::Outside,
                },
            },
        ],
    };
    for (time, expected) in [
        (at(-1, 1), StrokeAlign::Inside),
        (at(1, 2), StrokeAlign::Inside),
        (at(2000, 3000), StrokeAlign::Outside),
        (at(9, 1), StrokeAlign::Outside),
    ] {
        assert_eq!(
            sample_animation_channel_scene(&discrete, &time).unwrap(),
            SampledChannel::Discrete {
                parameter: DiscreteParameter::GraphicStrokeAlign,
                value: DiscreteValue::StrokeAlign { value: expected }
            }
        );
    }
}

#[test]
fn equal_endpoints_can_bump_or_undershoot_and_keep_raw_separate_from_physical() {
    let mut channel = scalar(
        ScalarParameter::Opacity,
        vec![key("a", at(0, 1), 0.0), key("b", at(1, 1), 0.0)],
    );
    keys_mut(&mut channel)[0].right_handle = Some(CurveHandle {
        dt: at(1, 3),
        dv: 2.0,
    });
    keys_mut(&mut channel)[1].left_handle = Some(CurveHandle {
        dt: at(-1, 3),
        dv: 2.0,
    });
    // Bernstein weights at u=.5: .125, .375, .375, .125 -> 1.5.
    let before = channel.clone();
    let (raw, physical) = sample(&channel, at(1, 2));
    close(raw, 1.5);
    // Classic's layer coefficient is not encoded alpha: the shader multiplies
    // it directly. Premature clamping would change the rendering semantics.
    close(physical, 1.5);
    assert_eq!(
        channel, before,
        "Flat is only a hint; sampling must not replace handles"
    );
    keys_mut(&mut channel)[0].right_handle.as_mut().unwrap().dv = -2.0;
    keys_mut(&mut channel)[1].left_handle.as_mut().unwrap().dv = -2.0;
    let (raw, physical) = sample(&channel, at(1, 2));
    close(raw, -1.5);
    close(physical, -1.5);
}

#[test]
fn time_value_bezier_solves_time_and_defaults_each_missing_handle_independently() {
    let mut channel = scalar(
        ScalarParameter::PositionXPx,
        vec![key("a", at(0, 1), 0.0), key("b", at(1, 1), 8.0)],
    );
    // x(u)=u^3, y(.5)=4; sampling at x=.125 must use u=.5, not .125.
    keys_mut(&mut channel)[0].right_handle = Some(CurveHandle {
        dt: at(0, 1),
        dv: 0.0,
    });
    keys_mut(&mut channel)[1].left_handle = Some(CurveHandle {
        dt: at(-1, 1),
        dv: 0.0,
    });
    close(sample(&channel, at(1, 8)).0, 4.0);
    keys_mut(&mut channel)[0].right_handle = None;
    keys_mut(&mut channel)[1].left_handle = Some(CurveHandle {
        dt: at(-1, 3),
        dv: 0.0,
    });
    // Default outgoing value=8/3; explicit incoming value=8.
    close(sample(&channel, at(1, 2)).0, 5.0);
    keys_mut(&mut channel)[1].left_handle = None;
    assert_eq!(sample(&channel, at(1, 4)), (2.0, 2.0));
}

#[test]
fn extrapolation_uses_edge_secants_and_a_single_key_always_holds() {
    let mut channel = scalar(
        ScalarParameter::PositionXPx,
        vec![key("a", at(1, 1), 2.0), key("b", at(3, 1), 6.0)],
    );
    keys_mut(&mut channel)[0].right_handle = Some(CurveHandle {
        dt: at(1, 1),
        dv: 50.0,
    });
    assert_eq!(sample(&channel, at(0, 1)), (0.0, 0.0));
    assert_eq!(sample(&channel, at(4, 1)), (8.0, 8.0));
    if let AnimationChannel::Scalar { extrapolation, .. } = &mut channel {
        *extrapolation = Extrapolation {
            before: ExtrapolationMode::Hold,
            after: ExtrapolationMode::Hold,
        };
    }
    assert_eq!(sample(&channel, at(0, 1)), (2.0, 2.0));
    assert_eq!(sample(&channel, at(4, 1)), (6.0, 6.0));
    let single = scalar(
        ScalarParameter::PositionXPx,
        vec![key("only", at(2, 1), 7.0)],
    );
    assert_eq!(sample(&single, at(-100, 1)), (7.0, 7.0));
    assert_eq!(sample(&single, at(100, 1)), (7.0, 7.0));
}

#[test]
fn legacy_native_lookup_preserves_32_segments_decimal_constants_and_exact_endpoints() {
    let mut channel = scalar(
        ScalarParameter::PositionXPx,
        vec![key("a", at(0, 1), 2.0), key("b", at(1, 1), 10.0)],
    );
    keys_mut(&mut channel)[0].segment_to_next = ScalarSegment::LegacyNormalizedBezier {
        curve: HistoricalVisualCubicBezier {
            x1: 1.0 / 3.0,
            y1: 0.0,
            x2: 2.0 / 3.0,
            y2: 0.0,
        },
    };
    // x=u, y=u^3. Old lookup's first interval interpolates 0 to
    // decimal12(1/32768)=.000030517578; halfway is .000015258789,
    // distinct from exact ease's 1/262144. Expected is hand-derived.
    close(sample(&channel, at(1, 64)).0, 2.000122070312);
    assert!((sample(&channel, at(1, 64)).0 - (2.0 + 8.0 / 262144.0)).abs() > 0.00008);
    keys_mut(&mut channel)[0].value = 2.123456789012345;
    keys_mut(&mut channel)[1].value = 10.987654321098765;
    assert_eq!(sample(&channel, at(0, 1)).0, 2.123456789012345);
    assert_eq!(sample(&channel, at(1, 1)).0, 10.987654321098765);
}

#[test]
fn adjacent_high_ticks_and_mixed_timebases_do_not_collapse_to_absolute_float_seconds() {
    let high = 9007199254740992;
    let channel = scalar(
        ScalarParameter::PositionXPx,
        vec![key("a", at(high, 24), 0.0), key("b", at(high + 2, 24), 8.0)],
    );
    assert_eq!(sample(&channel, at(2 * high + 1, 48)), (2.0, 2.0));
    let near_max = scalar(
        ScalarParameter::PositionXPx,
        vec![
            key("a", at(i64::MAX - 2, 120000), 0.0),
            key("b", at(i64::MAX, 120000), 8.0),
        ],
    );
    assert_eq!(sample(&near_max, at(i64::MAX - 1, 120000)), (4.0, 4.0));
    let mut high_curve = scalar(
        ScalarParameter::PositionXPx,
        vec![
            key("a", at(high, 120000), 0.0),
            key("b", at(high + 8, 120000), 8.0),
        ],
    );
    keys_mut(&mut high_curve)[0].right_handle = Some(CurveHandle {
        dt: at(0, 1),
        dv: 0.0,
    });
    keys_mut(&mut high_curve)[1].left_handle = Some(CurveHandle {
        dt: at(-8, 120000),
        dv: 0.0,
    });
    close(sample(&high_curve, at(high + 1, 120000)).0, 4.0);
    assert_eq!(
        element_local_time_scene(&at(2 * high + 1, 48), &at(high, 24)).unwrap(),
        at(1, 48)
    );
    assert_eq!(
        element_local_time_scene(&at(2 * high - 1, 48), &at(high, 24)).unwrap(),
        at(-1, 48)
    );
    assert_eq!(
        element_local_time_scene(&at(i64::MAX, 120000), &at(i64::MAX - 1, 120000)).unwrap(),
        at(1, 120000)
    );
}

#[test]
fn standalone_sampling_rejects_invalid_structure_even_at_an_unaffected_endpoint() {
    let good = scalar(
        ScalarParameter::PositionXPx,
        vec![key("a", at(0, 1), 0.0), key("b", at(1, 1), 1.0)],
    );
    let mut bad = good.clone();
    keys_mut(&mut bad).clear();
    rejected(&bad, "requires keys");
    bad = good.clone();
    keys_mut(&mut bad)[1].time = at(0, 2000);
    rejected(&bad, "strictly increasing");
    bad = good.clone();
    keys_mut(&mut bad)[1].id = "a".into();
    rejected(&bad, "duplicate");
    bad = good.clone();
    keys_mut(&mut bad)[1].value = f64::NAN;
    rejected(&bad, "finite");
    bad = good.clone();
    keys_mut(&mut bad)[0].right_handle = Some(CurveHandle {
        dt: at(-1, 3),
        dv: 0.0,
    });
    rejected(&bad, "wrong direction");
    keys_mut(&mut bad)[0].right_handle.as_mut().unwrap().dt = at(1001, 1000);
    rejected(&bad, "exceeds its adjacent segment");
    bad = good.clone();
    keys_mut(&mut bad)[1].time.timebase = 0;
    rejected(&bad, "timebase");
    bad = good.clone();
    keys_mut(&mut bad)[0].segment_to_next = ScalarSegment::LegacyNormalizedBezier {
        curve: HistoricalVisualCubicBezier {
            x1: 0.0,
            y1: -0.1,
            x2: 1.0,
            y2: 1.0,
        },
    };
    rejected(&bad, "Legacy normalized");
    bad = good;
    keys_mut(&mut bad)[0].segment_id = Some("same".into());
    keys_mut(&mut bad)[1].segment_id = Some("same".into());
    rejected(&bad, "duplicate");
}

#[test]
fn raw_nonfinite_and_nonpositive_layout_samples_fail_before_physical_projection() {
    let mut font = scalar(
        ScalarParameter::FontSizeCanvasPixels,
        vec![key("a", at(0, 1), 1.0), key("b", at(1, 1), 1.0)],
    );
    keys_mut(&mut font)[0].right_handle = Some(CurveHandle {
        dt: at(1, 3),
        dv: -4.0,
    });
    keys_mut(&mut font)[1].left_handle = Some(CurveHandle {
        dt: at(-1, 3),
        dv: -4.0,
    });
    let error = sample_animation_channel_scene(&font, &at(1, 2)).unwrap_err();
    assert!(error.message.contains("positive layout"));
    let channel = scalar(
        ScalarParameter::PositionXPx,
        vec![
            key("a", at(0, 1), -f64::MAX * 0.4),
            key("b", at(1, 1), f64::MAX * 0.4),
        ],
    );
    let error = sample_animation_channel_scene(&channel, &at(2, 1)).unwrap_err();
    assert!(error.message.contains("finite"));
}

#[test]
fn constant_time_value_bezier_is_exact_across_skew_time_controls_and_keeps_value_handles() {
    for value in [3.3843089645987314_f64, -17.25, 1.0] {
        let mut channel = scalar(ScalarParameter::PositionXPx,
            vec![key("a", at(0, 1), value), key("b", at(1, 1), value)]);
        keys_mut(&mut channel)[0].right_handle = Some(CurveHandle { dt: at(3, 4), dv: 0.0 });
        keys_mut(&mut channel)[1].left_handle = Some(CurveHandle { dt: at(-3, 4), dv: 0.0 });
        for i in 0..=1000 {
            let (raw, physical) = sample(&channel, at(i, 1000));
            assert_eq!(raw.to_bits(), value.to_bits());
            assert_eq!(physical.to_bits(), value.to_bits());
        }
        keys_mut(&mut channel)[0].right_handle.as_mut().unwrap().dv = 1.0;
        keys_mut(&mut channel)[1].left_handle.as_mut().unwrap().dv = 1.0;
        close(sample(&channel, at(1, 2)).0, value + 0.75);
    }
}
