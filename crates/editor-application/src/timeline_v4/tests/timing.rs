use super::*;
#[test]
fn trim_and_split_preserve_nonzero_source_trim_rate_gain_and_exact_public_apply() {
    let f = Fixture::new();
    let original = f.current.clone();
    for (edge, boundary, expected_start, trim_start, trim_end) in [
        (TimelineTrimEdgeV4::Left, t(2, 3), t(2, 3), t(3, 2), t(6, 1)),
        (
            TimelineTrimEdgeV4::Right,
            t(2, 1),
            t(1, 3),
            t(1, 1),
            t(13, 2),
        ),
    ] {
        let output = plan(
            &f,
            &f.request(TimelineCommandV4::Trim {
                target: selected(),
                edge,
                boundary,
                ripple: false,
            }),
        )
        .unwrap();
        let project = proposed(&f, &output);
        let e = &project.sequences[0].tracks[1].elements[0];
        eqtime(
            &e.timeline_range.start,
            expected_start.ticks.parse().unwrap(),
            expected_start.timebase,
        );
        eqtime(&e.timeline_range.duration, 5, 3);
        assert_eq!(e.content, f.e().content);
        assert_eq!(e.enabled, f.e().enabled);
        let ElementSourceSpanV4::ClassicTrimmed {
            trim_start: actual_start,
            trim_end: actual_end,
            source_duration,
        } = &e.source_span
        else {
            panic!()
        };
        eqtime(
            actual_start,
            trim_start.ticks.parse().unwrap(),
            trim_start.timebase,
        );
        eqtime(
            actual_end,
            trim_end.ticks.parse().unwrap(),
            trim_end.timebase,
        );
        assert_eq!(source_duration, &Some(t(10, 1)));
    }
    let request = f.request(split(t(4, 3), TimelineSplitRetainV4::Both, false));
    let output = plan(&f, &request).unwrap();
    assert_eq!(plan(&f, &request).unwrap(), output);
    let project = proposed(&f, &output);
    let elements = &project.sequences[0].tracks[1].elements;
    assert_eq!(
        elements.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
        ["voice", "right"]
    );
    for e in elements {
        eqtime(&e.timeline_range.duration, 1, 1);
        assert_eq!(e.content, f.e().content);
    }
    let ElementSourceSpanV4::ClassicTrimmed {
        trim_start,
        trim_end,
        ..
    } = &elements[0].source_span
    else {
        panic!()
    };
    eqtime(trim_start, 1, 1);
    eqtime(trim_end, 15, 2);
    let ElementSourceSpanV4::ClassicTrimmed {
        trim_start,
        trim_end,
        ..
    } = &elements[1].source_span
    else {
        panic!()
    };
    eqtime(trim_start, 5, 2);
    eqtime(trim_end, 6, 1);
    assert_eq!(f.current, original);
}
#[test]
fn fractional_tick_split_uses_total_minus_left_and_cap_wins_over_frame_snap() {
    let mut f = Fixture::new();
    // Classic independent oracle: timeline 4 ticks * 3/2 = 6 source
    // ticks. left 1 tick rounds to 2; right is 6-2=4, not round(3*1.5)=5.
    f.em().timeline_range = TimeRange {
        start: t(1, 3),
        duration: t(4, 120000),
    };
    f.em().source_span = ElementSourceSpanV4::ClassicTrimmed {
        trim_start: t(1, 1),
        trim_end: t(1079994, 120000),
        source_duration: Some(t(10, 1)),
    };
    let output = plan(
        &f,
        &f.request(split(t(40001, 120000), TimelineSplitRetainV4::Both, false)),
    )
    .unwrap();
    let project = proposed(&f, &output);
    let elements = &project.sequences[0].tracks[1].elements;
    eqtime(&elements[0].timeline_range.duration, 1, 120000);
    eqtime(&elements[1].timeline_range.duration, 3, 120000);
    let ElementSourceSpanV4::ClassicTrimmed {
        trim_start,
        trim_end,
        ..
    } = &elements[1].source_span
    else {
        panic!()
    };
    eqtime(trim_start, 120002, 120000);
    eqtime(trim_end, 1079994, 120000);
    // Nominal right request may round up by one, but the authored source
    // extent is intact and inverse-round(4 / 1.5) = 3 timeline ticks.
    let right_source = opencut_editor_core::project_v4::resolve_element_source_request_v4(
        &elements[1],
        Some(&project.assets[0]),
    )
    .unwrap();
    eqtime(&right_source.duration, 5, 120000);
    let mut f = Fixture::new();
    f.current.project.assets[0].duration = Some(t(5, 1));
    f.em().source_span = ElementSourceSpanV4::ClassicTrimmed {
        trim_start: t(1, 1),
        trim_end: t(3600, 120000),
        source_duration: Some(t(483600, 120000)),
    };
    let output = plan(
        &f,
        &f.request(TimelineCommandV4::Trim {
            target: selected(),
            edge: TimelineTrimEdgeV4::Right,
            boundary: t(9, 1),
            ripple: false,
        }),
    )
    .unwrap();
    let project = proposed(&f, &output);
    let e = &project.sequences[0].tracks[1].elements[0];
    // Source cap permits 2400 ticks (0.02s); snapping reaches the next
    // 4000-tick frame, then re-clamp keeps the legal non-frame cap.
    eqtime(&e.timeline_range.duration, 242400, 120000);
    let ElementSourceSpanV4::ClassicTrimmed { trim_end, .. } = &e.source_span else {
        panic!()
    };
    eqtime(trim_end, 0, 1);
}

#[test]
fn classic_nonintegral_frame_grid_keeps_original_ticks_instead_of_guessing_frame_rate() {
    let mut f = Fixture::new();
    let SettingsProfileV4::ClassicGlobal { settings } = &mut f.current.project.settings else {
        panic!()
    };
    settings.fps = FrameRate {
        numerator: 7,
        denominator: 3,
    };
    // Classic rust/time/frame_rate.rs returns None for 120000*3/7;
    // wasm/media-time.ts roundFrameTicks therefore returns the raw tick value.
    let output = plan(&f, &f.request(move_to(t(1, 1), "sound", false))).unwrap();
    eqtime(
        &proposed(&f, &output).sequences[0].tracks[1].elements[0]
            .timeline_range
            .start,
        1,
        1,
    );
    let output = plan(
        &f,
        &f.request(TimelineCommandV4::Trim {
            target: selected(),
            edge: TimelineTrimEdgeV4::Right,
            boundary: t(38, 15),
            ripple: false,
        }),
    )
    .unwrap();
    // Requested right delta .2 seconds survives rather than nearest 3/7s.
    eqtime(
        &proposed(&f, &output).sequences[0].tracks[1].elements[0]
            .timeline_range
            .duration,
        11,
        5,
    );
}
