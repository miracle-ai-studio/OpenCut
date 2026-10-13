use super::*;
fn text() -> TextV4 {
    TextV4 {
        content: "中文\nTitle".into(),
        font: FontBindingV4::Locked {
            font_id: "font".into(),
        },
        font_weight: FontWeightV4::Normal,
        font_style: FontStyleV4::Normal,
        decoration: TextDecorationV4::Underline,
        align: TextAlignV4::Center,
        font_size: FontSizeV4::ClassicHeight90 { value: 15.0 },
        color: ColorV4 {
            encoding: ColorEncodingV4::Srgb,
            red: 1.0,
            green: 0.5,
            blue: 0.2,
            alpha: 1.0,
        },
        letter_spacing_px: 1.5,
        line_height_multiplier: 1.25,
        background: TextBackgroundV4 {
            enabled: true,
            color: ColorV4 {
                encoding: ColorEncodingV4::Srgb,
                red: 1.0,
                green: 0.5,
                blue: 0.2,
                alpha: 1.0,
            },
            corner_radius_percent: 50.0,
            padding_x_at_size15: 3.0,
            padding_y_at_size15: 2.0,
            offset_x_px: -1.0,
            offset_y_px: 1.0,
        },
    }
}

fn fixture() -> Fixture {
    let mut f = Fixture::new();
    f.current.project.fonts.push(FontResourceV4 {
        id: "font".into(),
        family: "Stored face".into(),
        weight: 400,
        style: FontStyleV4::Normal,
        face_index: 0,
        content: AssetVersion {
            id: format!("av_{}", "b".repeat(64)),
            content_hash: "b".repeat(64),
            size: 1,
        },
        origin: ResourceOriginV4::LegacyLocator {
            uri: "/private/offline-font.ttf".into(),
        },
        license: None,
    });
    f.current.project.sequences[0].tracks[1].kind = TrackKindV4::Text;
    let e = f.em();
    e.content = ElementContentV4::Text { text: text() };
    e.visual = Some(visual());
    e.source_span = ElementSourceSpanV4::ClassicTrimmed {
        trim_start: t(1, 2),
        trim_end: t(3, 4),
        source_duration: None,
    };
    opencut_editor_core::project_v4::validate_project_v4(&f.current.project).unwrap();
    f
}
fn check_style(a: &TimelineElementV4, b: &TimelineElementV4) {
    assert_eq!(a.content, b.content);
    assert_eq!(a.visual, b.visual);
    assert_eq!(a.channels, b.channels);
}
#[test]
fn elements_text_split_retains_multiline_style_exact_time_and_deterministic_ids() {
    let f = fixture();
    let request = f.request(split(t(17, 15), TimelineSplitRetainV4::Both, false));
    let result = plan_elements(&f, &request).unwrap();
    assert_eq!(result, plan_elements(&f, &request).unwrap());
    let project = proposed(&f, &result);
    let parts = &project.sequences[0].tracks[1].elements;
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].id, "voice");
    assert_eq!(parts[1].id, "right");
    eqtime(&parts[0].timeline_range.duration, 4, 5);
    eqtime(&parts[1].timeline_range.start, 17, 15);
    eqtime(&parts[1].timeline_range.duration, 6, 5);
    for part in parts {
        check_style(part, f.e());
    }
    check_trims(&parts[0], t(1, 2), t(39, 20));
    check_trims(&parts[1], t(13, 10), t(3, 4));
    assert_eq!(
        plan(&f, &request).unwrap_err().code,
        "UNSUPPORTED_TIMELINE_PROFILE"
    );
    let mut collision = request.clone();
    if let TimelineCommandV4::Split { targets, .. } = &mut collision.command {
        targets[0].right_element_id = "font".into();
    }
    assert!(plan_elements(&f, &collision).is_err());
}
#[test]
fn elements_text_unbounded_trim_extends_and_preserves_source_trims() {
    let f = fixture();
    for (edge, boundary, start, duration, trim_start, trim_end) in [
        (
            TimelineTrimEdgeV4::Left,
            t(0, 1),
            t(0, 1),
            t(7, 3),
            t(1, 6),
            t(3, 4),
        ),
        (
            TimelineTrimEdgeV4::Right,
            t(4, 1),
            t(1, 3),
            t(11, 3),
            t(1, 2),
            t(0, 1),
        ),
    ] {
        let result = plan_elements(
            &f,
            &f.request(TimelineCommandV4::Trim {
                target: selected(),
                edge,
                boundary,
                ripple: false,
            }),
        )
        .unwrap();
        let project = proposed(&f, &result);
        let e = &project.sequences[0].tracks[1].elements[0];
        assert_eq!(
            time(&e.timeline_range.start)
                .unwrap()
                .cmp_exact(time(&start).unwrap()),
            std::cmp::Ordering::Equal
        );
        assert_eq!(
            time(&e.timeline_range.duration)
                .unwrap()
                .cmp_exact(time(&duration).unwrap()),
            std::cmp::Ordering::Equal
        );
        check_style(e, f.e());
        check_trims(e, trim_start, trim_end);
    }
}
#[test]
fn elements_text_move_tracks_locks_overlap_ripple_and_profile_identity() {
    let mut f = fixture();
    f.add_tail();
    f.current.project.sequences[0].tracks[1].elements[1].source_span = f.e().source_span.clone();
    f.second_track();
    let moved = plan_elements(&f, &f.request(move_to(t(1, 1), "other", false))).unwrap();
    let project = proposed(&f, &moved);
    let e = &project.sequences[0].tracks[2].elements[0];
    check_style(e, f.e());
    assert_eq!(e.source_span, f.e().source_span);
    assert!(plan_elements(&f, &f.request(move_to(t(3, 1), "sound", false))).is_err());
    f.current.project.sequences[0].tracks[2].locked = true;
    assert_eq!(
        plan_elements(&f, &f.request(move_to(t(1, 1), "other", false)))
            .unwrap_err()
            .code,
        "CONSTRAINT_VIOLATION"
    );
    let removed = plan_elements(
        &f,
        &f.request(TimelineCommandV4::Delete {
            targets: vec![selected()],
            ripple: true,
        }),
    )
    .unwrap();
    let project = proposed(&f, &removed);
    eqtime(
        &project.sequences[0].tracks[1].elements[0]
            .timeline_range
            .start,
        1,
        1,
    );
    assert_eq!(removed.decision.ripple_shifted_ids, vec!["tail"]);
    let media = Fixture::new();
    let request = media.request(move_to(t(1, 1), "sound", false));
    let old = plan(&media, &request).unwrap();
    let new = plan_elements(&media, &request).unwrap();
    assert_eq!(old.transaction, new.transaction);
    assert_ne!(old.original_request_digest, new.original_request_digest);
}
#[test]
fn elements_text_track_management_and_animation_rejection_are_explicit() {
    let mut f = fixture();
    let added = plan_elements(
        &f,
        &f.request(TimelineCommandV4::AddTrack {
            id: "extra-text".into(),
            name: "Text".into(),
            track_kind: TrackKindV4::Text,
        }),
    )
    .unwrap();
    assert_eq!(
        proposed(&f, &added).sequences[0]
            .tracks
            .last()
            .unwrap()
            .kind,
        TrackKindV4::Text
    );
    assert!(
        plan(
            &f,
            &f.request(TimelineCommandV4::AddTrack {
                id: "extra-text".into(),
                name: "Text".into(),
                track_kind: TrackKindV4::Text
            })
        )
        .is_err()
    );
    let mut channel = curve();
    if let AnimationChannelV4::Scalar { binding, keys, .. } = &mut channel {
        *binding = ScalarBindingV4::Property {
            parameter: ScalarParameterV4::Opacity,
        };
        keys[0].value = 0.75;
    }
    f.em().channels.push(channel);
    for command in [
        split(t(1, 1), TimelineSplitRetainV4::Both, false),
        TimelineCommandV4::Trim {
            target: selected(),
            edge: TimelineTrimEdgeV4::Right,
            boundary: t(4, 1),
            ripple: false,
        },
    ] {
        assert_eq!(
            plan_elements(&f, &f.request(command)).unwrap_err().code,
            "UNSUPPORTED_TIMELINE_ANIMATION"
        );
    }
    let moved = plan_elements(&f, &f.request(move_to(t(1, 1), "sound", false))).unwrap();
    check_style(
        &proposed(&f, &moved).sequences[0].tracks[1].elements[0],
        f.e(),
    );
}

fn curve() -> AnimationChannelV4 {
    AnimationChannelV4::Scalar {
        id: "channel".into(),
        target: ChannelTargetV4::Element {
            element_id: "voice".into(),
        },
        binding: ScalarBindingV4::Property {
            parameter: ScalarParameterV4::GainDecibels,
        },
        keys: vec![ScalarKeyV4 {
            id: "key".into(),
            segment_id: None,
            time: t(0, 1),
            value: -6.05,
            left_handle: None,
            right_handle: None,
            segment_to_next: ScalarSegmentV4::Linear,
            tangent_mode: TangentModeV4::Auto,
        }],
        extrapolation: ExtrapolationV4 {
            before: ExtrapolationModeV4::Hold,
            after: ExtrapolationModeV4::Hold,
        },
    }
}

fn check_trims(e: &TimelineElementV4, start: Time, end: Time) {
    let ElementSourceSpanV4::ClassicTrimmed {
        trim_start,
        trim_end,
        source_duration,
    } = &e.source_span
    else {
        panic!("classic trims preserved")
    };
    assert_eq!(
        time(trim_start).unwrap().cmp_exact(time(&start).unwrap()),
        std::cmp::Ordering::Equal
    );
    assert_eq!(
        time(trim_end).unwrap().cmp_exact(time(&end).unwrap()),
        std::cmp::Ordering::Equal
    );
    assert_eq!(*source_duration, None);
}

#[test]
fn elements_text_fixed_revision_revocation_and_neighbor_clamp_reuse_original_authority() {
    let mut f = fixture();
    let mut stale = f.request(move_to(t(1, 1), "sound", false));
    stale.expected_revision = "stale".into();
    assert_eq!(
        plan_elements(&f, &stale).unwrap_err().code,
        "REVISION_CONFLICT"
    );
    f.deny_after = Some(f.calls.get() + 1);
    assert_eq!(
        plan_elements(&f, &f.request(move_to(t(1, 1), "sound", false)))
            .unwrap_err()
            .code,
        "NOT_AUTHORIZED"
    );
    f.deny_after = None;
    f.add_tail();
    f.current.project.sequences[0].tracks[1].elements[1].source_span = f.e().source_span.clone();
    let result = plan_elements(
        &f,
        &f.request(TimelineCommandV4::Trim {
            target: selected(),
            edge: TimelineTrimEdgeV4::Right,
            boundary: t(20, 1),
            ripple: false,
        }),
    )
    .unwrap();
    let project = proposed(&f, &result);
    eqtime(
        &project.sequences[0].tracks[1].elements[0]
            .timeline_range
            .duration,
        8,
        3,
    );
    for command in [
        TimelineCommandV4::SetTrackEnabled {
            track_id: "sound".into(),
            enabled: false,
        },
        TimelineCommandV4::SetTrackMuted {
            track_id: "sound".into(),
            muted: true,
        },
        TimelineCommandV4::SetTrackLocked {
            track_id: "sound".into(),
            locked: true,
        },
    ] {
        let result = plan_elements(&f, &f.request(command)).unwrap();
        let p = proposed(&f, &result);
        assert_eq!(
            p.sequences[0].tracks[1].elements,
            f.current.project.sequences[0].tracks[1].elements
        );
    }
    f.second_track();
    let result = plan_elements(
        &f,
        &f.request(TimelineCommandV4::DeleteEmptyTrack {
            track_id: "other".into(),
        }),
    )
    .unwrap();
    assert_eq!(proposed(&f, &result).sequences[0].tracks.len(), 2);
}

#[test]
fn elements_text_split_rounds_source_once_and_preserves_exact_off_grid_timeline() {
    let mut f = fixture();
    f.em().timeline_range = TimeRange {
        start: t(0, 1),
        duration: t(10, 21),
    };
    let result = plan_elements(
        &f,
        &f.request(split(t(1, 7), TimelineSplitRetainV4::Both, false)),
    )
    .unwrap();
    let project = proposed(&f, &result);
    let parts = &project.sequences[0].tracks[1].elements;
    eqtime(&parts[0].timeline_range.duration, 1, 7);
    eqtime(&parts[1].timeline_range.start, 1, 7);
    eqtime(&parts[1].timeline_range.duration, 1, 3);
    // Independently calculated Classic expectations:
    // round(120000 / 7) = 17143; round(120000 * 10 / 21) = 57143;
    // right = 57143 - 17143 = 40000 (never independently rounded).
    check_trims(&parts[0], t(1, 2), t(130000, 120000));
    check_trims(&parts[1], t(77143, 120000), t(3, 4));
    let ElementSourceSpanV4::ClassicTrimmed { trim_end, .. } = &parts[0].source_span else {
        panic!()
    };
    let ElementSourceSpanV4::ClassicTrimmed { trim_start, .. } = &parts[1].source_span else {
        panic!()
    };
    let right = time(trim_end)
        .unwrap()
        .checked_sub(time(&t(3, 4)).unwrap())
        .unwrap();
    let left = time(trim_start)
        .unwrap()
        .checked_sub(time(&t(1, 2)).unwrap())
        .unwrap();
    assert_eq!(
        left.checked_add(right)
            .unwrap()
            .cmp_exact(time(&t(57143, 120000)).unwrap()),
        std::cmp::Ordering::Equal
    );
    for part in parts {
        check_style(part, f.e());
    }
    assert_eq!(
        plan(
            &f,
            &f.request(split(t(1, 7), TimelineSplitRetainV4::Both, false))
        )
        .unwrap_err()
        .code,
        "UNSUPPORTED_TIMELINE_PROFILE"
    );
}

#[test]
fn elements_text_legacy_source_range_keeps_exact_unrounded_split_semantics() {
    let mut f = fixture();
    f.em().timeline_range = TimeRange {
        start: t(0, 1),
        duration: t(10, 21),
    };
    f.em().source_span = ElementSourceSpanV4::LegacySourceRange {
        range: TimeRange {
            start: t(1, 2),
            duration: t(10, 21),
        },
    };
    let result = plan_elements(
        &f,
        &f.request(split(t(1, 7), TimelineSplitRetainV4::Both, false)),
    )
    .unwrap();
    let project = proposed(&f, &result);
    let parts = &project.sequences[0].tracks[1].elements;
    let ElementSourceSpanV4::LegacySourceRange { range } = &parts[1].source_span else {
        panic!()
    };
    eqtime(&range.start, 9, 14);
    eqtime(&range.duration, 1, 3);
}
