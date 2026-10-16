use super::*;
use opencut_editor_core::project_v4::{SampledChannelV4, sample_animation_channel_v4};
fn animated() -> Fixture {
    let mut f = Fixture::new();
    f.as_video();
    let e = f.em();
    e.channels = vec![AnimationChannelV4::Scalar {
        id: "position".into(),
        target: ChannelTargetV4::Element {
            element_id: e.id.clone(),
        },
        binding: ScalarBindingV4::Property {
            parameter: ScalarParameterV4::PositionXPx,
        },
        extrapolation: ExtrapolationV4 {
            before: ExtrapolationModeV4::Hold,
            after: ExtrapolationModeV4::Linear,
        },
        keys: vec![
            ScalarKeyV4 {
                id: "start-key".into(),
                segment_id: None,
                time: t(0, 1),
                value: 0.,
                left_handle: None,
                right_handle: Some(CurveHandleV4 {
                    dt: t(2, 3),
                    dv: 150.,
                }),
                segment_to_next: ScalarSegmentV4::Bezier,
                tangent_mode: TangentModeV4::Broken,
            },
            ScalarKeyV4 {
                id: "end-key".into(),
                segment_id: None,
                time: t(2, 1),
                value: 100.,
                left_handle: Some(CurveHandleV4 {
                    dt: t(-2, 3),
                    dv: 50.,
                }),
                right_handle: None,
                segment_to_next: ScalarSegmentV4::Linear,
                tangent_mode: TangentModeV4::Broken,
            },
        ],
    }];
    f
}
fn raw(c: &AnimationChannelV4, at: Time) -> f64 {
    let SampledChannelV4::Scalar { raw, .. } = sample_animation_channel_v4(c, &at).unwrap() else {
        panic!()
    };
    raw
}
#[test]
fn complete_split_keeps_classic_overshoot_samples_targets_and_unique_child_ids() {
    let f = animated();
    let request = f.request(split(t(4, 3), TimelineSplitRetainV4::Both, false));
    assert_eq!(
        plan_elements(&f, &request).unwrap_err().code,
        "UNSUPPORTED_TIMELINE_ANIMATION"
    );
    let result = plan_complete(&f, &request).unwrap();
    assert_eq!(result, plan_complete(&f, &request).unwrap());
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
    let AnimationChannelV4::Scalar {
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
        &ChannelTargetV4::Element {
            element_id: "right".into()
        }
    );
    assert!(
        keys.iter()
            .all(|k| k.id != "start-key" && k.id != "end-key")
    );
    assert_eq!(extrapolation.after, ExtrapolationModeV4::Linear);
    assert!(raw(&parts[0].channels[0], t(1, 1)) > 100.);
}
#[test]
fn complete_animated_trim_rebases_local_time_and_keeps_unedited_source() {
    let f = animated();
    let request = f.request(TimelineCommandV4::Trim {
        target: selected(),
        edge: TimelineTrimEdgeV4::Left,
        boundary: t(5, 6),
        ripple: false,
    });
    let result = plan_complete(&f, &request).unwrap();
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
            TrackKindV4::Effect
        } else {
            TrackKindV4::Graphic
        };
        f.em().content = if effect {
            ElementContentV4::EffectLayer {
                effect: EffectV4 {
                    id: "blur".into(),
                    enabled: true,
                    effect: EffectKindV4::ClassicBlur { intensity: 10. },
                },
            }
        } else {
            ElementContentV4::Graphic {
                graphic: GraphicV4 {
                    geometry: GraphicGeometryV4::Ellipse,
                    source_size: CanvasSizeV4 {
                        width: 80,
                        height: 60,
                    },
                    fill: ColorV4 {
                        encoding: ColorEncodingV4::Srgb,
                        red: 1.,
                        green: 0.,
                        blue: 0.,
                        alpha: 1.,
                    },
                    stroke: StrokeV4 {
                        color: ColorV4 {
                            encoding: ColorEncodingV4::Srgb,
                            red: 0.,
                            green: 0.,
                            blue: 0.,
                            alpha: 1.,
                        },
                        width_px: 0.,
                        align: StrokeAlignV4::Center,
                    },
                },
            }
        };
        f.em().visual = if effect { None } else { Some(visual()) };
        f.em().source_span = ElementSourceSpanV4::ClassicTrimmed {
            trim_start: t(0, 1),
            trim_end: t(0, 1),
            source_duration: None,
        };
        let move_request = f.request(move_to(t(2, 1), "sound", false));
        assert!(plan_elements(&f, &move_request).is_err());
        let result = plan_complete(&f, &move_request).unwrap();
        let proposed = proposed(&f, &result);
        eqtime(
            &proposed.sequences[0].tracks[1].elements[0]
                .timeline_range
                .start,
            2,
            1,
        );
        let request = f.request(TimelineCommandV4::SetTrackEnabled {
            track_id: "sound".into(),
            enabled: false,
        });
        assert!(plan_complete(&f, &request).is_ok());
    }
}
#[test]
fn complete_caption_mixed_batch_split_delete_ripple_keeps_style_and_anchors() {
    let mut f = animated();
    let mut track = f.current.project.sequences[0].tracks[0].clone();
    track.id = "captions".into();
    track.kind = TrackKindV4::Caption;
    f.current.project.sequences[0].tracks.push(track);
    let cue:CaptionV4=serde_json::from_value(serde_json::json!({"id":"cue","track_id":"captions","timeline_range":{"start":t(0,1),"duration":t(2,1)},"text":"Caption","source_anchor":null,"style":{"font":{"kind":"unresolved_family","family":"Arial","policy":{"mode":"strict"}},"font_size":{"unit":"classic_height90","value":15},"color":{"encoding":"srgb","red":1,"green":1,"blue":1,"alpha":1},"background_color":null,"max_width_px":null,"line_height_px":null,"text_align":"center","layout":"legacy_caption_v2"}})).unwrap();
    f.current.project.sequences[0].captions.push(cue.clone());
    let mut later = cue.clone();
    later.id = "later-cue".into();
    later.timeline_range.start = t(3, 1);
    f.current.project.sequences[0].captions.push(later);
    let target = TimelineTargetV4 {
        track_id: "captions".into(),
        element_id: "cue".into(),
    };
    let request = f.request(TimelineCommandV4::Delete {
        targets: vec![selected(), target],
        ripple: true,
    });
    let result = plan_complete(&f, &request).unwrap();
    let project = proposed(&f, &result);
    assert!(project.sequences[0].tracks[1].elements.is_empty());
    assert_eq!(project.sequences[0].captions.len(), 1);
    assert_eq!(project.sequences[0].captions[0].style, cue.style);
    eqtime(&project.sequences[0].captions[0].timeline_range.start, 1, 1);
}
