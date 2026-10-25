use super::*;
use opencut_contracts::historical_visual::HistoricalVisualCubicBezier;
fn time(ticks: i64, timebase: u32) -> Time {
    Time {
        ticks: ticks.to_string(),
        timebase,
    }
}
fn key(id: &str, t: i64, value: f64) -> ScalarKey {
    ScalarKey {
        id: id.into(),
        segment_id: None,
        time: time(t, 1),
        value,
        left_handle: None,
        right_handle: None,
        segment_to_next: ScalarSegment::Linear,
        tangent_mode: TangentMode::Broken,
    }
}
fn source() -> AnimationChannel {
    let mut left = key("first", 0, 0.12345678901234);
    left.segment_id = Some("old-segment".into());
    left.segment_to_next = ScalarSegment::LegacyNormalizedBezier {
        curve: HistoricalVisualCubicBezier {
            x1: 0.2,
            y1: 0.1,
            x2: 0.8,
            y2: 0.9,
        },
    };
    AnimationChannel::Scalar {
        id: "channel".into(),
        target: ChannelTarget::Element {
            element_id: "element".into(),
        },
        binding: ScalarBinding::Property {
            parameter: ScalarParameter::PositionXPx,
        },
        keys: vec![left, key("last", 3, 123.456789012345)],
        extrapolation: Extrapolation {
            before: ExtrapolationMode::Hold,
            after: ExtrapolationMode::Hold,
        },
    }
}
fn raw(channel: &AnimationChannel, time: &Time) -> f64 {
    let SampledChannel::Scalar { raw, .. } = sample_animation_channel_scene(channel, time).unwrap()
    else {
        panic!()
    };
    raw
}
#[test]
fn conversion_preserves_exact_endpoints_ids_and_bounds_interior_and_one_sided_jumps() {
    let source = source();
    let converted = convert_legacy_lookup_segments(
        &source,
        &["first".into()],
        &HashSet::from(["legacy_lookup_v1_0".into()]),
        1e-6,
    )
    .unwrap();
    assert_eq!(converted.converted_segments, 1);
    assert_eq!(converted.inserted_keys, 31);
    assert!(converted.error_bound > converted.max_absolute_error);
    let AnimationChannel::Scalar {
        keys,
        extrapolation,
        ..
    } = &converted.channel
    else {
        panic!()
    };
    let AnimationChannel::Scalar {
        keys: old,
        extrapolation: old_extrapolation,
        ..
    } = &source
    else {
        panic!()
    };
    assert_eq!(keys.len(), 33);
    assert_eq!(keys[0].id, old[0].id);
    assert_eq!(keys[0].segment_id, old[0].segment_id);
    assert_eq!(keys[0].time, old[0].time);
    assert_eq!(keys[0].value, old[0].value);
    assert_eq!(keys[32], old[1]);
    assert_eq!(extrapolation, old_extrapolation);
    assert_eq!(keys[1].id, "legacy_lookup_v1_1");
    assert_eq!(
        RationalTime::try_from(&keys[1].time).unwrap(),
        RationalTime::new(3, 32).unwrap()
    );
    for n in 0..=8192 {
        let at = time(n * 3, 8192);
        assert!((raw(&source, &at) - raw(&converted.channel, &at)).abs() <= converted.error_bound);
    }
    for at in [time(-1, 1), time(4, 1)] {
        assert_eq!(raw(&source, &at), raw(&converted.channel, &at));
    }
}
#[test]
fn tolerance_extrapolation_and_budget_reject_without_mutating_input() {
    let mut original = source();
    let saved = original.clone();
    assert!(
        convert_legacy_lookup_segments(&original, &["first".into()], &HashSet::new(), 0.0).is_err()
    );
    assert_eq!(original, saved);
    let AnimationChannel::Scalar { extrapolation, .. } = &mut original else {
        panic!()
    };
    extrapolation.after = ExtrapolationMode::Linear;
    assert!(
        convert_legacy_lookup_segments(&original, &["first".into()], &HashSet::new(), 1.)
            .unwrap_err()
            .message
            .contains("Hold")
    );
    for selected in [
        vec![],
        vec!["first".into(), "first".into()],
        vec!["missing".into()],
        vec!["last".into()],
    ] {
        assert!(convert_legacy_lookup_segments(&source(), &selected, &HashSet::new(), 1.).is_err());
    }
    let mut many = source();
    let AnimationChannel::Scalar { keys, .. } = &mut many else {
        panic!()
    };
    for i in 2..4090 {
        keys.push(key(&format!("key-{i}"), i as i64 + 2, 1.0));
    }
    assert!(
        convert_legacy_lookup_segments(&many, &["first".into()], &HashSet::new(), 1.)
            .unwrap_err()
            .message
            .contains("4096")
    );
}
#[test]
fn other_segment_handles_and_linear_extrapolation_remain_unchanged() {
    let mut source = source();
    let AnimationChannel::Scalar {
        keys,
        extrapolation,
        ..
    } = &mut source
    else {
        panic!()
    };
    keys.insert(0, key("before", -1, 0.));
    keys[0].time = time(0, 1);
    keys[1].time = time(1, 1);
    keys[2].time = time(4, 1);
    keys.push(key("after", 5, 200.));
    keys[0].segment_to_next = ScalarSegment::Bezier;
    keys[1].left_handle = Some(CurveHandle {
        dt: time(-1, 2),
        dv: -2.,
    });
    keys[2].segment_to_next = ScalarSegment::Bezier;
    keys[2].right_handle = Some(CurveHandle {
        dt: time(1, 2),
        dv: 10.,
    });
    *extrapolation = Extrapolation {
        before: ExtrapolationMode::Linear,
        after: ExtrapolationMode::Linear,
    };
    let converted =
        convert_legacy_lookup_segments(&source, &["first".into()], &HashSet::new(), 1e-6).unwrap();
    for at in [time(-10, 1), time(1, 2), time(9, 2), time(10, 1)] {
        assert_eq!(raw(&source, &at), raw(&converted.channel, &at));
    }
}

#[test]
fn exact_knots_near_large_clocks_are_preserved_or_atomically_refused_if_unrepresentable() {
    let mut source = source();
    let AnimationChannel::Scalar { keys, .. } = &mut source else {
        panic!()
    };
    let origin = 1_i64 << 48;
    keys[0].time = time(origin, 120000);
    keys[1].time = time(origin + 1024, 120000);
    let converted =
        convert_legacy_lookup_segments(&source, &["first".into()], &HashSet::new(), 1e-6).unwrap();
    for i in 0..=1024 {
        let at = time(origin + i, 120000);
        assert!((raw(&source, &at) - raw(&converted.channel, &at)).abs() <= converted.error_bound);
    }
    let AnimationChannel::Scalar { keys, .. } = &mut source else {
        panic!()
    };
    keys[0].time = time(0, 1);
    keys[1].time = time(1, u32::MAX);
    let unchanged = source.clone();
    assert!(
        convert_legacy_lookup_segments(&source, &["first".into()], &HashSet::new(), 1e-6).is_err()
    );
    assert_eq!(source, unchanged);
}
