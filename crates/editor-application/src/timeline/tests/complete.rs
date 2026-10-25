use super::*;
use opencut_editor_core::scene::{SampledChannel, sample_animation_channel_scene};
fn animated() -> Fixture {
    let mut f = Fixture::new();
    f.as_video();
    let e = f.em();
    e.channels = vec![AnimationChannel::Scalar {
        id: "position".into(),
        target: ChannelTarget::Element {
            element_id: e.id.clone(),
        },
        binding: ScalarBinding::Property {
            parameter: ScalarParameter::PositionXPx,
        },
        extrapolation: Extrapolation {
            before: ExtrapolationMode::Hold,
            after: ExtrapolationMode::Linear,
        },
        keys: vec![
            ScalarKey {
                id: "start-key".into(),
                segment_id: None,
                time: t(0, 1),
                value: 0.,
                left_handle: None,
                right_handle: Some(CurveHandle {
                    dt: t(2, 3),
                    dv: 150.,
                }),
                segment_to_next: ScalarSegment::Bezier,
                tangent_mode: TangentMode::Broken,
            },
            ScalarKey {
                id: "end-key".into(),
                segment_id: None,
                time: t(2, 1),
                value: 100.,
                left_handle: Some(CurveHandle {
                    dt: t(-2, 3),
                    dv: 50.,
                }),
                right_handle: None,
                segment_to_next: ScalarSegment::Linear,
                tangent_mode: TangentMode::Broken,
            },
        ],
    }];
    f
}
fn raw(c: &AnimationChannel, at: Time) -> f64 {
    let SampledChannel::Scalar { raw, .. } = sample_animation_channel_scene(c, &at).unwrap() else {
        panic!()
    };
    raw
}
#[test]
fn complete_group_move_keeps_exact_offsets_and_main_pin_without_changing_v4_alignment() {
    let mut f = Fixture::new();
    f.as_video();
    f.second_track();
    let mut second = f.e().clone();
    second.id = "second".into();
    second.timeline_range.start = t(7, 12);
    f.current.project.sequences[0].tracks[2]
        .elements
        .push(second);
    let command = |a, b, destination: &str| TimelineCommand::MoveSelection {
        moves: vec![
            TimelineMove {
                target: selected(),
                target_track_id: destination.into(),
                new_start: a,
            },
            TimelineMove {
                target: TimelineTarget {
                    track_id: "other".into(),
                    element_id: "second".into(),
                },
                target_track_id: "other".into(),
                new_start: b,
            },
        ],
        ripple: false,
    };
    let input = f.request(command(t(5, 6), t(13, 12), "sound"));
    let moved = proposed(&f, &plan(&f, &input).unwrap());
    eqtime(
        &moved.sequences[0].tracks[1].elements[0]
            .timeline_range
            .start,
        5,
        6,
    );
    eqtime(
        &moved.sequences[0].tracks[2].elements[0]
            .timeline_range
            .start,
        13,
        12,
    );
    let unchanged = f.request(command(t(1, 3), t(7, 12), "sound"));
    assert!(plan(&f, &unchanged).unwrap().transaction.is_none());
    // Empty-main pin moves the entire group together, even with zero raw delta.
    let pin = f.request(command(t(1, 3), t(7, 12), "main"));
    let pinned = proposed(&f, &plan(&f, &pin).unwrap());
    eqtime(
        &pinned.sequences[0].tracks[0].elements[0]
            .timeline_range
            .start,
        0,
        1,
    );
    eqtime(
        &pinned.sequences[0].tracks[2].elements[0]
            .timeline_range
            .start,
        1,
        4,
    );
    for (actual, original) in [
        (&moved.sequences[0].tracks[1].elements[0], f.e()),
        (
            &moved.sequences[0].tracks[2].elements[0],
            &f.current.project.sequences[0].tracks[2].elements[0],
        ),
    ] {
        let mut expected = original.clone();
        expected.timeline_range.start = actual.timeline_range.start.clone();
        assert_eq!(actual, &expected);
    }
}
#[test]
fn complete_split_keeps_classic_overshoot_samples_targets_and_unique_child_ids() {
    let f = animated();
    let request = f.request(split(t(4, 3), TimelineSplitRetain::Both, false));
    let result = plan(&f, &request).unwrap();
    assert_eq!(result, plan(&f, &request).unwrap());
    let project = proposed(&f, &result);
    let parts = &project.sequences[0].tracks[1].elements;
    assert_eq!(parts.len(), 2);
    for i in 0..=8 {
        let at = t(i, 4);
        let source = raw(&f.e().channels[0], at.clone());
        let actual = if i <= 4 {
            raw(&parts[0].channels[0], at)
        } else {
            raw(&parts[1].channels[0], t(i - 4, 4))
        };
        assert!((source - actual).abs() < 0.005, "{i}: {source} {actual}");
    }
    let AnimationChannel::Scalar {
        id,
        target,
        keys,
        extrapolation,
        ..
    } = &parts[1].channels[0]
    else {
        panic!()
    };
    assert_ne!(id, "position");
    assert_eq!(
        target,
        &ChannelTarget::Element {
            element_id: "right".into()
        }
    );
    assert!(
        keys.iter()
            .all(|k| k.id != "start-key" && k.id != "end-key")
    );
    assert_eq!(extrapolation.after, ExtrapolationMode::Linear);
    assert!(raw(&parts[0].channels[0], t(1, 1)) > 100.);
}
#[test]
fn complete_animated_trim_rebases_local_time_and_keeps_unedited_source() {
    let f = animated();
    let request = f.request(TimelineCommand::Trim {
        target: selected(),
        edge: TimelineTrimEdge::Left,
        boundary: t(5, 6),
        ripple: false,
    });
    let result = plan(&f, &request).unwrap();
    let project = proposed(&f, &result);
    let e = &project.sequences[0].tracks[1].elements[0];
    for i in 0..=5 {
        let at = t(i, 4);
        assert!(
            (raw(&e.channels[0], at.clone())
                - raw(
                    &f.e().channels[0],
                    time(&at)
                        .unwrap()
                        .checked_add(time(&t(1, 2)).unwrap())
                        .unwrap()
                        .into()
                ))
            .abs()
                < 0.01
        );
    }
    assert_eq!(f.e().channels[0], animated().e().channels[0]);
}
#[test]
fn complete_graphic_and_effect_layer_move_delete_and_all_track_controls() {
    for effect in [false, true] {
        let mut f = animated();
        f.em().channels.clear();
        f.current.project.sequences[0].tracks[1].kind = if effect {
            SceneTrackKind::Effect
        } else {
            SceneTrackKind::Graphic
        };
        f.em().content = if effect {
            ElementContent::EffectLayer {
                effect: SceneEffect {
                    id: "blur".into(),
                    enabled: true,
                    effect: EffectKind::ClassicBlur { intensity: 10. },
                },
            }
        } else {
            ElementContent::Graphic {
                graphic: Graphic {
                    geometry: GraphicGeometry::Ellipse,
                    source_size: CanvasSize {
                        width: 80,
                        height: 60,
                    },
                    fill: Color {
                        encoding: ColorEncoding::Srgb,
                        red: 1.,
                        green: 0.,
                        blue: 0.,
                        alpha: 1.,
                    },
                    stroke: Stroke {
                        color: Color {
                            encoding: ColorEncoding::Srgb,
                            red: 0.,
                            green: 0.,
                            blue: 0.,
                            alpha: 1.,
                        },
                        width_px: 0.,
                        align: StrokeAlign::Center,
                    },
                },
            }
        };
        f.em().visual = if effect { None } else { Some(visual()) };
        f.em().source_span = ElementSourceSpan::ClassicTrimmed {
            trim_start: t(0, 1),
            trim_end: t(0, 1),
            source_duration: None,
        };
        let move_request = f.request(move_to(t(2, 1), "sound", false));
        let result = plan(&f, &move_request).unwrap();
        let proposed = proposed(&f, &result);
        eqtime(
            &proposed.sequences[0].tracks[1].elements[0]
                .timeline_range
                .start,
            2,
            1,
        );
        let request = f.request(TimelineCommand::SetTrackEnabled {
            track_id: "sound".into(),
            enabled: false,
        });
        assert!(plan(&f, &request).is_ok());
    }
}
#[test]
fn complete_caption_mixed_batch_split_delete_ripple_keeps_style_and_anchors() {
    let mut f = animated();
    let mut track = f.current.project.sequences[0].tracks[0].clone();
    track.id = "captions".into();
    track.kind = SceneTrackKind::Caption;
    f.current.project.sequences[0].tracks.push(track);
    let cue:Caption=serde_json::from_value(serde_json::json!({"id":"cue","track_id":"captions","timeline_range":{"start":t(0,1),"duration":t(2,1)},"text":"Caption","source_anchor":null,"style":{"font":{"kind":"unresolved_family","family":"Arial","policy":{"mode":"strict"}},"font_size":{"unit":"classic_height90","value":15},"color":{"encoding":"srgb","red":1,"green":1,"blue":1,"alpha":1},"background_color":null,"max_width_px":null,"line_height_px":null,"text_align":"center","layout":"legacy_caption_v2"}})).unwrap();
    f.current.project.sequences[0].captions.push(cue.clone());
    let mut later = cue.clone();
    later.id = "later-cue".into();
    later.timeline_range.start = t(3, 1);
    f.current.project.sequences[0].captions.push(later);
    let target = TimelineTarget {
        track_id: "captions".into(),
        element_id: "cue".into(),
    };
    let baseline = f.current.clone();
    let mut locked = f.current.project.sequences[0]
        .tracks
        .last()
        .unwrap()
        .clone();
    locked.id = "locked-captions".into();
    locked.locked = true;
    f.current.project.sequences[0].tracks.push(locked);
    let mut unrelated = cue.clone();
    unrelated.id = "locked-cue".into();
    unrelated.track_id = "locked-captions".into();
    unrelated.timeline_range.start = t(8, 1);
    f.current.project.sequences[0]
        .captions
        .push(unrelated.clone());
    for retain in [TimelineSplitRetain::Both, TimelineSplitRetain::Right] {
        let request = f.request(TimelineCommand::Split {
            targets: vec![TimelineSplit {
                target: target.clone(),
                right_element_id: "right-cue".into(),
            }],
            at: t(1, 1),
            retain,
            ripple: true,
        });
        let result = plan(&f, &request).unwrap();
        let actual = proposed(&f, &result);
        let captions = &actual.sequences[0].captions;
        assert_eq!(
            captions.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            if retain == TimelineSplitRetain::Both {
                vec!["cue", "right-cue", "later-cue", "locked-cue"]
            } else {
                vec!["right-cue", "later-cue", "locked-cue"]
            }
        );
        let right = captions.iter().find(|c| c.id == "right-cue").unwrap();
        assert_eq!(right.style, cue.style);
        assert_eq!(captions.last().unwrap(), &unrelated);
        eqtime(
            &right.timeline_range.start,
            if retain == TimelineSplitRetain::Both {
                1
            } else {
                0
            },
            1,
        );
        eqtime(
            &captions
                .iter()
                .find(|c| c.id == "later-cue")
                .unwrap()
                .timeline_range
                .start,
            if retain == TimelineSplitRetain::Both {
                3
            } else {
                2
            },
            1,
        );
    }
    f.current = baseline;
    let request = f.request(TimelineCommand::Delete {
        targets: vec![selected(), target],
        ripple: true,
    });
    let result = plan(&f, &request).unwrap();
    let project = proposed(&f, &result);
    assert!(project.sequences[0].tracks[1].elements.is_empty());
    assert_eq!(project.sequences[0].captions.len(), 1);
    assert_eq!(project.sequences[0].captions[0].style, cue.style);
    eqtime(&project.sequences[0].captions[0].timeline_range.start, 1, 1);
}
