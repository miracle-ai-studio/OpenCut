use super::*;
#[test]
fn simultaneous_moves_preserve_fields_noop_order_and_track_local_ripple() {
    let mut f = Fixture::new();
    f.add_tail();
    f.second_track();
    let before = f.current.clone();
    let output = plan(&f, &f.request(move_to(t(40000, 120000), "sound", true))).unwrap();
    assert!(output.transaction.is_none());
    assert_eq!(f.current, before);
    let output = plan(&f, &f.request(move_to(t(1, 1), "other", true))).unwrap();
    let project = proposed(&f, &output);
    assert!(output.decision.ripple_shifted_ids.is_empty());
    eqtime(
        &project.sequences[0].tracks[1].elements[0]
            .timeline_range
            .start,
        3,
        1,
    );
    let mut expected = f.e().clone();
    expected.timeline_range.start = t(120000, 120000);
    assert_eq!(project.sequences[0].tracks[2].elements[0], expected);
    let output = plan(
        &f,
        &f.request(TimelineCommandV4::Delete {
            targets: vec![selected()],
            ripple: true,
        }),
    )
    .unwrap();
    let project = proposed(&f, &output);
    assert_eq!(output.decision.ripple_shifted_ids, ["tail"]);
    eqtime(
        &project.sequences[0].tracks[1].elements[0]
            .timeline_range
            .start,
        1,
        1,
    );
    assert_eq!(
        project.sequences[0].tracks[2],
        f.current.project.sequences[0].tracks[2]
    );
    // The split-created right half joins precisely the interval freed by left
    // duration shrink, so ordinary split+ripple closes no gap.
    let output = plan(
        &f,
        &f.request(split(t(4, 3), TimelineSplitRetainV4::Both, true)),
    )
    .unwrap();
    let project = proposed(&f, &output);
    assert!(output.decision.ripple_shifted_ids.is_empty());
    assert_eq!(
        project.sequences[0].tracks[1]
            .elements
            .iter()
            .map(|e| e.id.as_str())
            .collect::<Vec<_>>(),
        ["voice", "right", "tail"]
    );
    let output = plan(
        &f,
        &f.request(split(t(4, 3), TimelineSplitRetainV4::Left, true)),
    )
    .unwrap();
    let project = proposed(&f, &output);
    eqtime(
        &project.sequences[0].tracks[1].elements[1]
            .timeline_range
            .start,
        2,
        1,
    );
}
#[test]
fn image_split_rebinds_nested_ids_deterministically_without_altering_visual_payload() {
    let mut f = Fixture::new();
    f.as_video();
    f.current.project.assets[0].kind = AssetKind::Image;
    f.current.project.assets[0].duration = None;
    f.current.project.assets[0].metadata.audio = AudioPresenceV4::Absent {};
    f.current.project.sequences[0].tracks[1].kind = TrackKindV4::Graphic;
    f.em().content = ElementContentV4::Image {
        asset_id: "asset".into(),
    };
    f.em().source_span = ElementSourceSpanV4::ClassicTrimmed {
        trim_start: t(1, 1),
        trim_end: t(0, 1),
        source_duration: None,
    };
    let v = f.em().visual.as_mut().unwrap();
    v.effects.push(EffectV4 {
        id: "effect".into(),
        enabled: false,
        effect: EffectKindV4::ClassicBlur { intensity: 10.0 },
    });
    let point = |id: &str, x, y| FreeformPointV4 {
        id: id.into(),
        x,
        y,
        in_x: x - 0.1,
        in_y: y,
        out_x: x + 0.1,
        out_y: y,
    };
    v.masks.push(MaskV4 {
        id: "mask".into(),
        enabled: false,
        inverted: true,
        shape: MaskShapeV4::Freeform {
            path: FreeformMaskV4 {
                points: vec![
                    point("point-a", -0.5, -0.5),
                    point("point-b", 0.5, -0.5),
                    point("point-c", 0.0, 0.5),
                ],
                closed: true,
                center_x: 0.1,
                center_y: 0.0,
                rotation_degrees: 30.0,
                scale: 1.0,
            },
        },
        feather: MaskFeatherV4::ClassicPixels { value: 3.0 },
        stroke: StrokeV4 {
            color: ColorV4 {
                encoding: ColorEncodingV4::Srgb,
                red: 1.0,
                green: 0.0,
                blue: 0.0,
                alpha: 1.0,
            },
            width_px: 2.0,
            align: StrokeAlignV4::Outside,
        },
    });
    let request = f.request(split(t(4, 3), TimelineSplitRetainV4::Both, false));
    let output = plan(&f, &request).unwrap();
    assert_eq!(output, plan(&f, &request).unwrap());
    let project = proposed(&f, &output);
    let a = project.sequences[0].tracks[1].elements[0]
        .visual
        .as_ref()
        .unwrap();
    let b = project.sequences[0].tracks[1].elements[1]
        .visual
        .as_ref()
        .unwrap();
    assert_eq!(a, f.e().visual.as_ref().unwrap());
    assert_eq!(a.transform, b.transform);
    assert_eq!(a.effects[0].effect, b.effects[0].effect);
    assert_ne!(a.effects[0].id, b.effects[0].id);
    assert_ne!(a.masks[0].id, b.masks[0].id);
    let (MaskShapeV4::Freeform { path: a }, MaskShapeV4::Freeform { path: b }) =
        (&a.masks[0].shape, &b.masks[0].shape)
    else {
        panic!()
    };
    for (a, b) in a.points.iter().zip(&b.points) {
        assert_ne!(a.id, b.id);
        let mut expected = a.clone();
        expected.id = b.id.clone();
        assert_eq!(&expected, b);
    }
    for id in ["asset", "point-a", "scene", "main"] {
        let mut request = request.clone();
        if let TimelineCommandV4::Split { targets, .. } = &mut request.command {
            targets[0].right_element_id = id.into();
        }
        assert_eq!(plan(&f, &request).unwrap_err().code, "INVALID_ARGUMENT");
    }
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
#[test]
fn locks_animation_live_authority_and_fixed_head_reject_before_any_commit() {
    let mut f = Fixture::new();
    f.add_tail();
    f.em().channels.push(curve());
    let moved = plan(&f, &f.request(move_to(t(0, 1), "sound", false))).unwrap();
    let project = proposed(&f, &moved);
    assert_eq!(
        project.sequences[0].tracks[1]
            .elements
            .last()
            .unwrap()
            .channels,
        f.e().channels
    );
    for command in [
        split(t(4, 3), TimelineSplitRetainV4::Both, false),
        TimelineCommandV4::Trim {
            target: selected(),
            edge: TimelineTrimEdgeV4::Right,
            boundary: t(2, 1),
            ripple: false,
        },
    ] {
        assert_eq!(
            plan(&f, &f.request(command)).unwrap_err().code,
            "UNSUPPORTED_TIMELINE_ANIMATION"
        );
    }
    f.current.project.sequences[0].tracks[1].locked = true;
    assert_eq!(
        plan(&f, &f.request(move_to(t(1, 3), "sound", false)))
            .unwrap_err()
            .code,
        "CONSTRAINT_VIOLATION"
    );
    let unlocked = plan(
        &f,
        &f.request(TimelineCommandV4::SetTrackLocked {
            track_id: "sound".into(),
            locked: false,
        }),
    )
    .unwrap();
    assert!(!proposed(&f, &unlocked).sequences[0].tracks[1].locked);
    let f = Fixture::new();
    let mut request = f.request(move_to(t(1, 1), "sound", false));
    request.expected_revision = "old".into();
    assert_eq!(plan(&f, &request).unwrap_err().code, "REVISION_CONFLICT");
    for allowed in [0, 1] {
        let mut f = Fixture::new();
        f.deny_after = Some(allowed);
        assert_eq!(
            plan(&f, &f.request(move_to(t(1, 1), "sound", false)))
                .unwrap_err()
                .code,
            "NOT_AUTHORIZED"
        );
        assert_eq!(f.reads.get(), usize::from(allowed > 0));
    }
    let mut f = Fixture::new();
    f.add_tail();
    assert_eq!(
        plan(&f, &f.request(move_to(t(2, 1), "sound", false)))
            .unwrap_err()
            .code,
        "INVALID_PROJECT"
    );
}
#[test]
fn empty_media_track_management_uses_distinct_tag_and_preserves_full_existing_track() {
    let f = Fixture::new();
    let request = f.request(TimelineCommandV4::AddTrack {
        id: "extra".into(),
        name: "Extra".into(),
        track_kind: TrackKindV4::Audio,
    });
    let wire = serde_json::to_value(&request).unwrap();
    assert_eq!(wire["command"]["kind"], "add_track");
    assert_eq!(wire["command"]["track_kind"], "audio");
    assert_eq!(
        serde_json::from_value::<TimelineEditRequestV4>(wire).unwrap(),
        request
    );
    let output = plan(&f, &request).unwrap();
    let p = proposed(&f, &output);
    assert_eq!(p.sequences[0].tracks.len(), 3);
    assert!(p.sequences[0].tracks[2].elements.is_empty());
    assert!(!p.sequences[0].tracks[2].locked);
    let mut f = Fixture::new();
    f.second_track();
    let output = plan(
        &f,
        &f.request(TimelineCommandV4::DeleteEmptyTrack {
            track_id: "other".into(),
        }),
    )
    .unwrap();
    assert_eq!(proposed(&f, &output).sequences[0].tracks.len(), 2);
    for track in ["main", "sound"] {
        assert_eq!(
            plan(
                &f,
                &f.request(TimelineCommandV4::DeleteEmptyTrack {
                    track_id: track.into()
                })
            )
            .unwrap_err()
            .code,
            "INVALID_ARGUMENT"
        );
    }
    let output = plan(
        &f,
        &f.request(TimelineCommandV4::SetTrackMuted {
            track_id: "sound".into(),
            muted: true,
        }),
    )
    .unwrap();
    let mut expected = f.current.project.clone();
    expected.sequences[0].tracks[1].muted = true;
    assert_eq!(proposed(&f, &output), expected);
}
#[test]
fn transitions_survive_valid_seams_and_track_states_but_invalid_cut_is_removed() {
    let mut f = Fixture::new();
    f.as_video();
    f.em().timeline_range = TimeRange {
        start: t(0, 1),
        duration: t(2, 1),
    };
    f.em().source_span = ElementSourceSpanV4::ClassicTrimmed {
        trim_start: t(1, 1),
        trim_end: t(7, 1),
        source_duration: Some(t(10, 1)),
    };
    if let ElementContentV4::Video { retime, .. } = &mut f.em().content {
        retime.rate = RationalRateV4 {
            numerator: 1,
            denominator: 1,
        };
    }
    let mut incoming = f.e().clone();
    incoming.id = "incoming".into();
    incoming.timeline_range.start = t(2, 1);
    f.current.project.sequences[0].tracks[1]
        .elements
        .push(incoming);
    f.current.project.sequences[0]
        .transitions
        .push(TransitionV4 {
            id: "cut".into(),
            outgoing_element_id: "voice".into(),
            incoming_element_id: "incoming".into(),
            duration: t(1, 1),
            transition: TransitionKindV4::LegacyCrossfadeV3,
        });
    let move_pair = TimelineCommandV4::MoveSelection {
        moves: vec![
            TimelineMoveV4 {
                target: selected(),
                target_track_id: "sound".into(),
                new_start: t(1, 1),
            },
            TimelineMoveV4 {
                target: TimelineTargetV4 {
                    track_id: "sound".into(),
                    element_id: "incoming".into(),
                },
                target_track_id: "sound".into(),
                new_start: t(3, 1),
            },
        ],
        ripple: false,
    };
    for command in [
        move_pair,
        TimelineCommandV4::SetTrackMuted {
            track_id: "sound".into(),
            muted: true,
        },
        TimelineCommandV4::Trim {
            target: selected(),
            edge: TimelineTrimEdgeV4::Left,
            boundary: t(1, 2),
            ripple: false,
        },
    ] {
        let output = plan(&f, &f.request(command)).unwrap();
        assert!(output.decision.removed_transition_ids.is_empty());
        assert_eq!(
            proposed(&f, &output).sequences[0].transitions,
            f.current.project.sequences[0].transitions
        );
    }
    let output = plan(
        &f,
        &f.request(TimelineCommandV4::Trim {
            target: selected(),
            edge: TimelineTrimEdgeV4::Right,
            boundary: t(3, 2),
            ripple: false,
        }),
    )
    .unwrap();
    assert_eq!(output.decision.removed_transition_ids, ["cut"]);
    assert!(proposed(&f, &output).sequences[0].transitions.is_empty());
}
