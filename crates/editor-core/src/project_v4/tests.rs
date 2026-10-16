use super::*;
use opencut_contracts::{AssetVersion, scene_bookmarks::SceneBookmarkV1};

mod font_synthesis;

fn t(ticks: i64, timebase: u32) -> Time {
    Time {
        ticks: ticks.to_string(),
        timebase,
    }
}
fn span(start: Time, duration: Time) -> TimeRange {
    TimeRange { start, duration }
}
fn rgba(red: f64, green: f64, blue: f64) -> ColorV4 {
    ColorV4 {
        encoding: ColorEncodingV4::Srgb,
        red,
        green,
        blue,
        alpha: 1.0,
    }
}
fn version(digit: char) -> AssetVersion {
    let hash = digit.to_string().repeat(64);
    AssetVersion {
        id: format!("av_{hash}"),
        content_hash: hash,
        size: 1,
    }
}
fn audio() -> AudioStyleV4 {
    AudioStyleV4 {
        stream: None,
        gain: GainV4::Decibels { value: 20.0 },
        muted: false,
    }
}
fn retime(n: u32, d: u32) -> RetimeV4 {
    RetimeV4 {
        rate: RationalRateV4 {
            numerator: n,
            denominator: d,
        },
        maintain_pitch: true,
        mapping: RetimeMappingV4::ExactRational,
    }
}
fn style() -> VisualStyleV4 {
    VisualStyleV4 {
        profile: VisualProfileV4::ClassicCanvasV31,
        transform: TransformV4 {
            position_x_px: 0.0,
            position_y_px: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation_degrees: 0.0,
        },
        opacity: 1.0,
        blend_mode: BlendModeV4::Normal,
        effects: vec![],
        masks: vec![],
    }
}
fn track(id: &str, kind: TrackKindV4, elements: Vec<TimelineElementV4>) -> TrackV4 {
    TrackV4 {
        id: id.into(),
        name: id.into(),
        kind,
        enabled: true,
        muted: false,
        locked: false,
        opacity: 1.0,
        gain: GainV4::LinearLegacy { value: 1.0 },
        elements,
        channels: vec![],
    }
}
fn video() -> TimelineElementV4 {
    TimelineElementV4 {
        id: "video-element".into(),
        name: "Video".into(),
        timeline_range: span(t(1, 2), t(2, 1)),
        enabled: true,
        source_span: ElementSourceSpanV4::LegacySourceRange {
            range: span(t(1, 1), t(2, 1)),
        },
        content: ElementContentV4::Video {
            asset_id: "media".into(),
            video_stream: None,
            source_audio_enabled: false,
            audio: audio(),
            retime: retime(1, 1),
        },
        visual: Some(style()),
        channels: vec![],
    }
}
fn project() -> ProjectDocumentV4 {
    ProjectDocumentV4 {
        schema_version: 4,
        id: "project".into(),
        name: "Project".into(),
        default_sequence_id: "scene".into(),
        settings: SettingsProfileV4::ClassicGlobal {
            settings: ProjectSettingsV4 {
                fps: FrameRate {
                    numerator: 60000,
                    denominator: 1001,
                },
                canvas_size: CanvasSizeV4 {
                    width: 160,
                    height: 90,
                },
                canvas_size_mode: CanvasSizeModeV4::Preset,
                last_custom_canvas_size: None,
                original_canvas_size: None,
                background: BackgroundV4::MainTrackBlur { intensity: 200.0 },
            },
        },
        assets: vec![AssetV4 {
            id: "media".into(),
            name: "Source".into(),
            kind: AssetKind::Video,
            version: version('a'),
            duration: Some(t(5, 1)),
            origin: ResourceOriginV4::LocalImport {
                original_name: "source.mp4".into(),
            },
            metadata: MediaMetadataV4 {
                mime_type: Some("video/mp4".into()),
                intrinsic_size: Some(CanvasSizeV4 {
                    width: 160,
                    height: 90,
                }),
                frame_rate: Some(FrameRate {
                    numerator: 30000,
                    denominator: 1001,
                }),
                rotation_degrees: None,
                audio: AudioPresenceV4::Present {
                    metadata: Some(AudioMetadataV4 {
                        sample_rate: 48000,
                        channels: 2,
                        channel_layout: Some("stereo".into()),
                    }),
                },
            },
            license: None,
        }],
        fonts: vec![FontResourceV4 {
            id: "font".into(),
            family: "Fixture".into(),
            weight: 400,
            style: FontStyleV4::Normal,
            face_index: 0,
            content: version('b'),
            origin: ResourceOriginV4::LocalImport {
                original_name: "fixture.ttf".into(),
            },
            license: None,
        }],
        sequences: vec![SequenceV4 {
            id: "scene".into(),
            name: "Scene".into(),
            main_track_id: "main".into(),
            tracks: vec![track("main", TrackKindV4::Video, vec![video()])],
            captions: vec![],
            transitions: vec![],
            bookmarks: vec![],
        }],
    }
}
fn first(p: &mut ProjectDocumentV4) -> &mut TimelineElementV4 {
    &mut p.sequences[0].tracks[0].elements[0]
}
fn key(id: &str, at: Time, value: f64) -> ScalarKeyV4 {
    ScalarKeyV4 {
        id: id.into(),
        segment_id: None,
        time: at,
        value,
        left_handle: None,
        right_handle: None,
        segment_to_next: ScalarSegmentV4::Bezier,
        tangent_mode: TangentModeV4::Broken,
    }
}
fn channel(
    target: ChannelTargetV4,
    parameter: ScalarParameterV4,
    keys: Vec<ScalarKeyV4>,
) -> AnimationChannelV4 {
    AnimationChannelV4::Scalar {
        id: "channel".into(),
        target,
        binding: ScalarBindingV4::Property { parameter },
        keys,
        extrapolation: ExtrapolationV4 {
            before: ExtrapolationModeV4::Linear,
            after: ExtrapolationModeV4::Hold,
        },
    }
}
fn rejected(project: &ProjectDocumentV4, message: &str) {
    let error = validate_project_v4(project).unwrap_err();
    assert_eq!(error.code, "INVALID_PROJECT");
    assert!(
        error.message.contains(message),
        "unexpected validation failure: {}",
        error.message
    );
}

#[test]
fn explicit_format_scope_covers_every_scene_without_hidden_overrides() {
    let mut p = project();
    p.sequences.push(SequenceV4 {
        id: "other".into(),
        name: "Other".into(),
        main_track_id: "other-main".into(),
        tracks: vec![track("other-main", TrackKindV4::Video, vec![])],
        captions: vec![],
        transitions: vec![],
        bookmarks: vec![],
    });
    validate_project_v4(&p).unwrap();
    let format = |id: &str, rate| LegacySequenceFormatV4 {
        sequence_id: id.into(),
        fps: FrameRate {
            numerator: rate,
            denominator: 1,
        },
        canvas_size: CanvasSizeV4 {
            width: 320,
            height: 180,
        },
        background: BackgroundV4::Color {
            color: rgba(0.0, 0.0, 0.0),
        },
    };
    p.settings = SettingsProfileV4::LegacyPerScene {
        formats: vec![format("scene", 24), format("other", 60)],
    };
    validate_project_v4(&p).unwrap();
    if let SettingsProfileV4::LegacyPerScene { formats } = &mut p.settings {
        formats.pop();
    }
    rejected(&p, "every sequence");
    if let SettingsProfileV4::LegacyPerScene { formats } = &mut p.settings {
        formats.push(format("scene", 60));
    }
    rejected(&p, "distinct existing");
}

#[test]
fn exact_fractional_retime_preserves_nonzero_clocks_and_rejects_drift_or_resource_escape() {
    let mut p = project();
    let e = first(&mut p);
    e.timeline_range = span(t(1001, 30000), t(1001, 30000));
    e.source_span = ElementSourceSpanV4::LegacySourceRange {
        range: span(t(1001, 24000), t(1001, 15000)),
    };
    if let ElementContentV4::Video { retime, .. } = &mut e.content {
        retime.rate.numerator = 2;
    }
    let before = p.clone();
    validate_project_v4(&p).unwrap();
    assert_eq!(p, before, "validation must not quantize stored times");
    first(&mut p).timeline_range.duration = t(1002, 30000);
    rejected(&p, "disagree with retime");
    p = before;
    first(&mut p).source_span = ElementSourceSpanV4::LegacySourceRange {
        range: span(t(5, 1), t(1001, 15000)),
    };
    rejected(&p, "exceeds resource");
}

#[test]
fn classic_half_tick_rounding_accepts_inverse_rate_edit_but_not_arbitrary_inconsistency() {
    let mut p = project();
    p.assets[0].duration = Some(t(120001, 120000));
    let e = first(&mut p);
    e.timeline_range.duration = t(60001, 120000);
    e.source_span = ElementSourceSpanV4::ClassicTrimmed {
        trim_start: t(0, 1),
        trim_end: t(0, 1),
        source_duration: Some(t(120001, 120000)),
    };
    if let ElementContentV4::Video { retime, .. } = &mut e.content {
        retime.rate.numerator = 2;
        retime.mapping = RetimeMappingV4::ClassicRoundedTicks { timebase: 120000 };
    }
    // Classic's 120000 timebase preserves this inverse-rounded rate edit. Its
    // last visible integer local tick (60000) maps to source tick 120000, still
    // inside the source; the request duration itself is not a bounded read span.
    let before = p.clone();
    validate_project_v4(&p).unwrap();
    assert_eq!(p, before);
    let e = &p.sequences[0].tracks[0].elements[0];
    let request = resolve_element_source_request_v4(e, Some(&p.assets[0])).unwrap();
    assert_eq!(
        duration(&request.duration).unwrap(),
        RationalTime::new(120002, 120000).unwrap()
    );
    assert_eq!(p.assets[0].duration, Some(t(120001, 120000)));
    let mut wrong_asset = p.assets[0].clone();
    wrong_asset.id = "other-resource".into();
    assert!(resolve_element_source_request_v4(e, Some(&wrong_asset)).is_err());
    assert!(resolve_element_source_request_v4(e, None).is_err());
    first(&mut p).timeline_range.duration = t(60002, 120000);
    rejected(&p, "both declared rounding directions");
    p = before;
    first(&mut p).source_span = ElementSourceSpanV4::LegacySourceRange {
        range: span(t(0, 1), t(120002, 120000)),
    };
    if let ElementContentV4::Video { retime, .. } = &mut first(&mut p).content {
        retime.mapping = RetimeMappingV4::ExactRational;
    }
    rejected(&p, "exceeds resource duration");
}

#[test]
fn gain_units_do_not_silently_reinterpret_old_linear_values() {
    assert_eq!(
        gain_amplitude(&GainV4::LinearLegacy { value: 0.0 }).unwrap(),
        0.0
    );
    assert_eq!(
        gain_amplitude(&GainV4::LinearLegacy { value: 4.0 }).unwrap(),
        4.0
    );
    assert_eq!(
        gain_amplitude(&GainV4::Decibels { value: 20.0 }).unwrap(),
        10.0
    );
    assert!((gain_amplitude(&GainV4::Decibels { value: -60.0 }).unwrap() - 0.001).abs() < 1e-12);
    let mut p = project();
    first(&mut p).channels.push(channel(
        ChannelTargetV4::Element {
            element_id: "video-element".into(),
        },
        ScalarParameterV4::GainLinearLegacy,
        vec![key("key", t(0, 1), 1.0)],
    ));
    rejected(&p, "unit or type");
    if let ElementContentV4::Video { audio, .. } = &mut first(&mut p).content {
        audio.gain = GainV4::LinearLegacy { value: 1.0 };
    }
    validate_project_v4(&p).unwrap();
}

#[test]
fn effect_reference_and_parameter_are_bound_to_the_owning_element() {
    let mut p = project();
    first(&mut p)
        .visual
        .as_mut()
        .unwrap()
        .effects
        .push(EffectV4 {
            id: "blur".into(),
            enabled: true,
            effect: EffectKindV4::ClassicBlur { intensity: 200.0 },
        });
    first(&mut p).channels.push(channel(
        ChannelTargetV4::Effect {
            element_id: "video-element".into(),
            effect_id: "blur".into(),
        },
        ScalarParameterV4::ClassicBlurIntensity,
        vec![key("blur-key", t(1, 2), 200.0)],
    ));
    validate_project_v4(&p).unwrap();
    if let AnimationChannelV4::Scalar { binding, .. } = &mut first(&mut p).channels[0] {
        *binding = ScalarBindingV4::Property {
            parameter: ScalarParameterV4::LegacyBlurRadiusPx,
        };
    }
    rejected(&p, "unit or type");
    if let AnimationChannelV4::Scalar { target, .. } = &mut first(&mut p).channels[0] {
        *target = ChannelTargetV4::Effect {
            element_id: "elsewhere".into(),
            effect_id: "blur".into(),
        };
    }
    rejected(&p, "another element");
}

#[test]
fn actual_tangent_overshoot_survives_while_duplicate_exact_times_and_wrong_direction_fail() {
    let mut p = project();
    let mut a = key("a", t(1, 2), 1.0);
    a.tangent_mode = TangentModeV4::Flat;
    a.right_handle = Some(CurveHandleV4 {
        dt: t(1, 3),
        dv: 0.275,
    });
    let mut b = key("b", t(3, 2), 1.0);
    b.left_handle = Some(CurveHandleV4 {
        dt: t(-1, 3),
        dv: 0.275,
    });
    first(&mut p).channels.push(channel(
        ChannelTargetV4::Element {
            element_id: "video-element".into(),
        },
        ScalarParameterV4::ScaleX,
        vec![a, b],
    ));
    validate_project_v4(&p).unwrap();
    if let AnimationChannelV4::Scalar { keys, .. } = &mut first(&mut p).channels[0] {
        keys[1].time = t(1000, 2000);
    }
    rejected(&p, "strictly ordered");
    if let AnimationChannelV4::Scalar { keys, .. } = &mut first(&mut p).channels[0] {
        keys[1].time = t(3, 2);
        keys[0].right_handle.as_mut().unwrap().dt = t(-1, 3);
    }
    rejected(&p, "wrong direction");
}

#[test]
fn track_channels_use_sequence_time_and_cannot_hide_in_element_local_storage() {
    let mut p = project();
    p.sequences[0].tracks[0].channels.push(channel(
        ChannelTargetV4::Track {
            track_id: "main".into(),
        },
        ScalarParameterV4::Opacity,
        vec![key("track-key", t(9, 4), 0.5)],
    ));
    // Local clip duration is 2s, but scene duration is 2.5s.
    validate_project_v4(&p).unwrap();
    let channel = p.sequences[0].tracks[0].channels.pop().unwrap();
    first(&mut p).channels.push(channel);
    rejected(&p, "cannot target");
}

#[test]
fn stable_ids_cas_and_element_track_types_reject_cross_scope_aliasing() {
    let mut p = project();
    p.assets[0].version.content_hash = "c".repeat(64);
    rejected(&p, "av_<content_hash>");
    p = project();
    p.fonts[0].id = "media".into();
    rejected(&p, "duplicate V4 identity");
    p = project();
    p.sequences[0]
        .tracks
        .push(track("audio-track", TrackKindV4::Audio, vec![video()]));
    p.sequences[0].tracks[1].elements[0].id = "wrong-kind".into();
    rejected(&p, "incompatible");
}

#[test]
fn audio_presence_preserves_unknown_history_without_treating_absence_as_a_stream() {
    let mut p = project();
    p.assets[0].metadata.audio = AudioPresenceV4::Absent {};
    if let ElementContentV4::Video {
        source_audio_enabled,
        ..
    } = &mut first(&mut p).content
    {
        *source_audio_enabled = true;
    }
    // Classic's default preference is not an assertion that probing found audio.
    validate_project_v4(&p).unwrap();
    let mut sound = video();
    sound.id = "sound".into();
    sound.visual = None;
    sound.content = ElementContentV4::Audio {
        asset_id: "media".into(),
        audio: audio(),
        retime: retime(1, 1),
    };
    p.sequences[0]
        .tracks
        .push(track("audio-track", TrackKindV4::Audio, vec![sound]));
    rejected(&p, "known absent audio");
    // This is structural preservation; an actual detach/restore must still probe.
    p.assets[0].metadata.audio = AudioPresenceV4::Unknown {};
    validate_project_v4(&p).unwrap();
    p.assets[0].metadata.audio = AudioPresenceV4::Present { metadata: None };
    validate_project_v4(&p).unwrap();
    p.assets[0].metadata.audio = AudioPresenceV4::Present {
        metadata: Some(AudioMetadataV4 {
            sample_rate: 48000,
            channels: 2,
            channel_layout: None,
        }),
    };
    validate_project_v4(&p).unwrap();
}

#[test]
fn pinned_caption_references_locked_cas_font_and_bookmarks_keep_their_owning_scene() {
    let mut p = project();
    p.sequences[0].captions.push(CaptionV4 {
        id: "caption".into(),
        track_id: "caption-track".into(),
        timeline_range: span(t(1001, 30000), t(1, 1)),
        text: "中文 caption".into(),
        source_anchor: Some(opencut_contracts::CaptionSourceAnchor {
            asset_id: "media".into(),
            source_range: span(t(1, 1), t(1, 1)),
        }),
        style: CaptionStyleV4 {
            font: FontBindingV4::Locked {
                font_id: "font".into(),
            },
            font_size: FontSizeV4::CanvasPixels { value: 24.0 },
            color: rgba(1.0, 1.0, 1.0),
            background_color: None,
            max_width_px: Some(120),
            line_height_px: Some(28.0),
            text_align: TextAlignV4::Center,
            layout: CaptionLayoutProfileV4::PinnedCaptionV4,
        },
    });
    p.sequences[0]
        .tracks
        .push(track("caption-track", TrackKindV4::Caption, vec![]));
    p.sequences[0].bookmarks.push(SceneBookmarkV1 {
        id: "bookmark".into(),
        sequence_id: "scene".into(),
        time: t(1001, 30000),
        duration: Some(t(0, 1)),
        note: Some("note".into()),
        color: Some("#abcdef".into()),
    });
    validate_project_v4(&p).unwrap();
    p.sequences[0].captions[0].track_id = "main".into();
    rejected(&p, "owning caption track");
    p.sequences[0].captions[0].track_id = "caption-track".into();
    p.assets[0].metadata.audio = AudioPresenceV4::Absent {};
    rejected(&p, "known absent audio");
    p.sequences[0].captions[0].style.layout = CaptionLayoutProfileV4::LegacyCaptionV2;
    validate_project_v4(&p).unwrap();
    p.sequences[0].captions[0]
        .source_anchor
        .as_mut()
        .unwrap()
        .source_range
        .duration = t(5, 1);
    rejected(&p, "exceeds resource duration");
    p.sequences[0].captions[0]
        .source_anchor
        .as_mut()
        .unwrap()
        .source_range
        .duration = t(1, 1);
    p.assets[0].metadata.audio = AudioPresenceV4::Unknown {};
    p.sequences[0].captions[0].style.layout = CaptionLayoutProfileV4::PinnedCaptionV4;
    validate_project_v4(&p).unwrap();
    p.sequences[0].captions[0].style.font = FontBindingV4::Locked {
        font_id: "missing".into(),
    };
    rejected(&p, "font reference");
    p.sequences[0].captions[0].style.font = FontBindingV4::UnresolvedFamily {
        family: "Legacy".into(),
        policy: FontResolutionPolicy::Strict {},
    };
    rejected(&p, "Pinned caption");
    p.sequences[0].captions[0].style.layout = CaptionLayoutProfileV4::LegacyCaptionV2;
    validate_project_v4(&p).unwrap();
    p.sequences[0].bookmarks[0].sequence_id = "elsewhere".into();
    rejected(&p, "owning sequence");
}

#[test]
fn time_overflow_and_overlap_reject_the_whole_aggregate_without_mutation() {
    let mut p = project();
    first(&mut p).timeline_range = span(t(i64::MAX, 1), t(1, 1));
    let before = p.clone();
    rejected(&p, "outside the public representation");
    assert_eq!(p, before);
    p = project();
    let mut duplicate = video();
    duplicate.id = "overlap".into();
    p.sequences[0].tracks[0].elements.push(duplicate);
    rejected(&p, "cannot overlap");

    // V3 graphic layers preserve their original vector order even when their
    // images/shapes overlap; this does not relax Classic edit-track rules.
    p.assets[0].kind = AssetKind::Image;
    p.assets[0].duration = None;
    p.assets[0].metadata.frame_rate = None;
    p.assets[0].metadata.audio = AudioPresenceV4::Absent {};
    p.sequences[0].tracks[0].kind = TrackKindV4::Graphic;
    for e in &mut p.sequences[0].tracks[0].elements {
        e.content = ElementContentV4::Image {
            asset_id: "media".into(),
        };
        e.visual.as_mut().unwrap().profile = VisualProfileV4::LegacyCompositionV3;
    }
    p.sequences[0].main_track_id = "empty-main".into();
    p.sequences[0]
        .tracks
        .push(track("empty-main", TrackKindV4::Video, vec![]));
    validate_project_v4(&p).unwrap();
    first(&mut p).visual.as_mut().unwrap().profile = VisualProfileV4::ClassicCanvasV31;
    rejected(&p, "cannot overlap");
}

#[test]
fn freeform_handles_and_stable_mask_channels_are_validated_without_flattening_geometry() {
    let mut p = project();
    let point = |id: &str, x, y| FreeformPointV4 {
        id: id.into(),
        x,
        y,
        in_x: x - 0.1,
        in_y: y,
        out_x: x + 0.1,
        out_y: y,
    };
    first(&mut p).visual.as_mut().unwrap().masks.push(MaskV4 {
        id: "path-mask".into(),
        enabled: true,
        inverted: false,
        shape: MaskShapeV4::Freeform {
            path: FreeformMaskV4 {
                points: vec![
                    point("point-a", -0.5, -0.5),
                    point("point-b", 0.5, -0.5),
                    point("point-c", 0.0, 0.5),
                ],
                closed: true,
                center_x: -0.25,
                center_y: 0.0,
                rotation_degrees: 30.0,
                scale: 1.0,
            },
        },
        feather: MaskFeatherV4::ClassicPixels { value: 3.0 },
        stroke: StrokeV4 {
            color: rgba(1.0, 0.0, 0.0),
            width_px: 2.0,
            align: StrokeAlignV4::Outside,
        },
    });
    first(&mut p).channels.push(channel(
        ChannelTargetV4::Mask {
            element_id: "video-element".into(),
            mask_id: "path-mask".into(),
        },
        ScalarParameterV4::MaskFeatherClassicPixels,
        vec![key("feather-key", t(1, 2), 4.0)],
    ));
    let before = p.clone();
    validate_project_v4(&p).unwrap();
    assert_eq!(p, before);
    if let AnimationChannelV4::Scalar { binding, .. } = &mut first(&mut p).channels[0] {
        *binding = ScalarBindingV4::Property {
            parameter: ScalarParameterV4::MaskWidth,
        };
    }
    rejected(&p, "unit or type");
    p = before;
    if let MaskShapeV4::Freeform { path } =
        &mut first(&mut p).visual.as_mut().unwrap().masks[0].shape
    {
        path.points[1].in_x = f64::NAN;
    }
    rejected(&p, "point or handle");
}

#[test]
fn generated_trim_extent_text_units_and_fractional_shape_counts_keep_distinct_meanings() {
    let mut p = project();
    // A concrete bold italic face must match the title's request.
    p.fonts[0].weight = 700;
    p.fonts[0].style = FontStyleV4::Italic;
    let mut title = video();
    title.id = "title".into();
    title.name = "Title".into();
    title.source_span = ElementSourceSpanV4::ClassicTrimmed {
        trim_start: t(1, 1),
        trim_end: t(1, 1),
        source_duration: Some(t(4, 1)),
    };
    title.content = ElementContentV4::Text {
        text: TextV4 {
            content: "中文\nTitle".into(),
            font: FontBindingV4::Locked {
                font_id: "font".into(),
            },
            font_weight: FontWeightV4::Bold,
            font_style: FontStyleV4::Italic,
            decoration: TextDecorationV4::Underline,
            align: TextAlignV4::Center,
            font_size: FontSizeV4::ClassicHeight90 { value: 15.0 },
            color: rgba(1.0, 1.0, 1.0),
            letter_spacing_px: 1.0,
            line_height_multiplier: 1.25,
            background: TextBackgroundV4 {
                enabled: true,
                color: rgba(0.0, 0.0, 0.0),
                corner_radius_percent: 50.0,
                padding_x_at_size15: 3.0,
                padding_y_at_size15: 2.0,
                offset_x_px: -1.0,
                offset_y_px: 1.0,
            },
        },
    };
    title.channels.push(channel(
        ChannelTargetV4::Element {
            element_id: "title".into(),
        },
        ScalarParameterV4::FontSizeClassicHeight90,
        vec![key("font-key", t(0, 1), 15.0)],
    ));
    p.sequences[0]
        .tracks
        .push(track("text-track", TrackKindV4::Text, vec![title]));
    let mut shape = video();
    shape.id = "polygon".into();
    shape.name = "Polygon".into();
    shape.source_span = ElementSourceSpanV4::ClassicTrimmed {
        trim_start: t(0, 1),
        trim_end: t(0, 1),
        source_duration: None,
    };
    shape.content = ElementContentV4::Graphic {
        graphic: GraphicV4 {
            geometry: GraphicGeometryV4::Polygon {
                sides: 4.5,
                corner_radius_percent: 25.0,
            },
            source_size: CanvasSizeV4 {
                width: 200,
                height: 100,
            },
            fill: rgba(1.0, 0.0, 0.0),
            stroke: StrokeV4 {
                color: rgba(0.0, 0.0, 1.0),
                width_px: 4.0,
                align: StrokeAlignV4::Inside,
            },
        },
    };
    p.sequences[0]
        .tracks
        .push(track("graphic-track", TrackKindV4::Graphic, vec![shape]));
    validate_project_v4(&p).unwrap();
    p.fonts[0].weight = 400;
    rejected(&p, "does not match requested weight and style");
    p.fonts[0].weight = 700;
    p.fonts[0].style = FontStyleV4::Normal;
    rejected(&p, "does not match requested weight and style");
    p.fonts[0].style = FontStyleV4::Italic;
    if let AnimationChannelV4::Scalar { binding, .. } =
        &mut p.sequences[0].tracks[1].elements[0].channels[0]
    {
        *binding = ScalarBindingV4::Property {
            parameter: ScalarParameterV4::FontSizeCanvasPixels,
        };
    }
    rejected(&p, "unit or type");
}

#[test]
fn custom_canvas_memory_survives_native_first_visual_canvas_override() {
    let mut value = project();
    let SettingsProfileV4::ClassicGlobal { settings } = &mut value.settings else {
        panic!()
    };
    settings.canvas_size_mode = CanvasSizeModeV4::Custom;
    settings.last_custom_canvas_size = Some(CanvasSizeV4 {
        width: 720,
        height: 1280,
    });
    settings.canvas_size = CanvasSizeV4 {
        width: 1920,
        height: 1080,
    };
    validate_project_v4(&value).unwrap();
    let SettingsProfileV4::ClassicGlobal { settings } = &mut value.settings else {
        panic!()
    };
    settings.last_custom_canvas_size = Some(CanvasSizeV4 {
        width: 0,
        height: 1280,
    });
    assert!(validate_project_v4(&value).is_err());
}

#[test]
fn classic_open_freeform_drafts_preserve_empty_and_single_point_geometry() {
    for enabled in [true, false] {
        for count in 0..=2 {
            let mut p = project();
            first(&mut p).visual.as_mut().unwrap().masks.push(MaskV4 {
                id: "draft-mask".into(),
                enabled,
                inverted: true,
                shape: MaskShapeV4::Freeform {
                    path: FreeformMaskV4 {
                        points: (0..count)
                            .map(|i| FreeformPointV4 {
                                id: format!("draft-point-{i}"),
                                x: i as f64,
                                y: 0.,
                                in_x: -0.5,
                                in_y: 0.25,
                                out_x: 0.75,
                                out_y: -0.125,
                            })
                            .collect(),
                        closed: false,
                        center_x: 0.25,
                        center_y: -0.5,
                        rotation_degrees: 42.,
                        scale: 1.25,
                    },
                },
                feather: MaskFeatherV4::ClassicPixels { value: 3. },
                stroke: StrokeV4 {
                    color: rgba(1., 0., 0.),
                    width_px: 2.,
                    align: StrokeAlignV4::Center,
                },
            });
            let before = p.clone();
            validate_project_v4(&p).unwrap();
            assert_eq!(before, p);
            let MaskShapeV4::Freeform { path } =
                &mut first(&mut p).visual.as_mut().unwrap().masks[0].shape
            else {
                panic!()
            };
            path.closed = true;
            rejected(&p, "enough path points");
            p = before;
            if count > 0 {
                let MaskShapeV4::Freeform { path } =
                    &mut first(&mut p).visual.as_mut().unwrap().masks[0].shape
                else {
                    panic!()
                };
                path.points[0].out_y = f64::INFINITY;
                rejected(&p, "point or handle");
            }
        }
    }
}
