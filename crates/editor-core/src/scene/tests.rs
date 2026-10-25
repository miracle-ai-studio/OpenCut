use super::*;
use opencut_contracts::{AssetVersion, scene_bookmarks::SceneBookmark};

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
fn rgba(red: f64, green: f64, blue: f64) -> Color {
    Color {
        encoding: ColorEncoding::Srgb,
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
fn audio() -> AudioStyle {
    AudioStyle {
        stream: None,
        gain: Gain::Decibels { value: 20.0 },
        muted: false,
    }
}
fn retime(n: u32, d: u32) -> Retime {
    Retime {
        rate: RationalRate {
            numerator: n,
            denominator: d,
        },
        maintain_pitch: true,
        mapping: RetimeMapping::ExactRational,
    }
}
fn style() -> VisualStyle {
    VisualStyle {
        profile: VisualProfile::ClassicCanvasV31,
        transform: Transform {
            position_x_px: 0.0,
            position_y_px: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation_degrees: 0.0,
        },
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: vec![],
        masks: vec![],
    }
}
fn track(id: &str, kind: SceneTrackKind, elements: Vec<TimelineElement>) -> SceneTrack {
    SceneTrack {
        id: id.into(),
        name: id.into(),
        kind,
        enabled: true,
        muted: false,
        locked: false,
        opacity: 1.0,
        gain: Gain::LinearLegacy { value: 1.0 },
        elements,
        channels: vec![],
    }
}
fn video() -> TimelineElement {
    TimelineElement {
        id: "video-element".into(),
        name: "Video".into(),
        timeline_range: span(t(1, 2), t(2, 1)),
        enabled: true,
        source_span: ElementSourceSpan::LegacySourceRange {
            range: span(t(1, 1), t(2, 1)),
        },
        content: ElementContent::Video {
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
fn project() -> EditorDocument {
    EditorDocument {
        schema_version: 4,
        id: "project".into(),
        name: "Project".into(),
        default_sequence_id: "scene".into(),
        settings: SettingsProfile::ClassicGlobal {
            settings: ProjectSettings {
                fps: FrameRate {
                    numerator: 60000,
                    denominator: 1001,
                },
                canvas_size: CanvasSize {
                    width: 160,
                    height: 90,
                },
                canvas_size_mode: CanvasSizeMode::Preset,
                last_custom_canvas_size: None,
                original_canvas_size: None,
                background: Background::MainTrackBlur { intensity: 200.0 },
            },
        },
        assets: vec![ProjectAsset {
            id: "media".into(),
            name: "Source".into(),
            kind: AssetKind::Video,
            version: version('a'),
            duration: Some(t(5, 1)),
            origin: ResourceOrigin::LocalImport {
                original_name: "source.mp4".into(),
            },
            metadata: ProjectMediaMetadata {
                mime_type: Some("video/mp4".into()),
                intrinsic_size: Some(CanvasSize {
                    width: 160,
                    height: 90,
                }),
                frame_rate: Some(FrameRate {
                    numerator: 30000,
                    denominator: 1001,
                }),
                rotation_degrees: None,
                audio: AudioPresence::Present {
                    metadata: Some(AudioMetadata {
                        sample_rate: 48000,
                        channels: 2,
                        channel_layout: Some("stereo".into()),
                    }),
                },
            },
            license: None,
        }],
        fonts: vec![FontResource {
            id: "font".into(),
            family: "Fixture".into(),
            weight: 400,
            style: FontStyle::Normal,
            face_index: 0,
            content: version('b'),
            origin: ResourceOrigin::LocalImport {
                original_name: "fixture.ttf".into(),
            },
            license: None,
        }],
        sequences: vec![Scene {
            id: "scene".into(),
            name: "Scene".into(),
            main_track_id: "main".into(),
            tracks: vec![track("main", SceneTrackKind::Video, vec![video()])],
            captions: vec![],
            transitions: vec![],
            bookmarks: vec![],
        }],
    }
}
fn first(p: &mut EditorDocument) -> &mut TimelineElement {
    &mut p.sequences[0].tracks[0].elements[0]
}
fn key(id: &str, at: Time, value: f64) -> ScalarKey {
    ScalarKey {
        id: id.into(),
        segment_id: None,
        time: at,
        value,
        left_handle: None,
        right_handle: None,
        segment_to_next: ScalarSegment::Bezier,
        tangent_mode: TangentMode::Broken,
    }
}
fn channel(
    target: ChannelTarget,
    parameter: ScalarParameter,
    keys: Vec<ScalarKey>,
) -> AnimationChannel {
    AnimationChannel::Scalar {
        id: "channel".into(),
        target,
        binding: ScalarBinding::Property { parameter },
        keys,
        extrapolation: Extrapolation {
            before: ExtrapolationMode::Linear,
            after: ExtrapolationMode::Hold,
        },
    }
}
fn rejected(project: &EditorDocument, message: &str) {
    let error = validate_project_scene(project).unwrap_err();
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
    p.sequences.push(Scene {
        id: "other".into(),
        name: "Other".into(),
        main_track_id: "other-main".into(),
        tracks: vec![track("other-main", SceneTrackKind::Video, vec![])],
        captions: vec![],
        transitions: vec![],
        bookmarks: vec![],
    });
    validate_project_scene(&p).unwrap();
    let format = |id: &str, rate| LegacySequenceFormat {
        sequence_id: id.into(),
        fps: FrameRate {
            numerator: rate,
            denominator: 1,
        },
        canvas_size: CanvasSize {
            width: 320,
            height: 180,
        },
        background: Background::Color {
            color: rgba(0.0, 0.0, 0.0),
        },
    };
    p.settings = SettingsProfile::LegacyPerScene {
        formats: vec![format("scene", 24), format("other", 60)],
    };
    validate_project_scene(&p).unwrap();
    if let SettingsProfile::LegacyPerScene { formats } = &mut p.settings {
        formats.pop();
    }
    rejected(&p, "every sequence");
    if let SettingsProfile::LegacyPerScene { formats } = &mut p.settings {
        formats.push(format("scene", 60));
    }
    rejected(&p, "distinct existing");
}

#[test]
fn exact_fractional_retime_preserves_nonzero_clocks_and_rejects_drift_or_resource_escape() {
    let mut p = project();
    let e = first(&mut p);
    e.timeline_range = span(t(1001, 30000), t(1001, 30000));
    e.source_span = ElementSourceSpan::LegacySourceRange {
        range: span(t(1001, 24000), t(1001, 15000)),
    };
    if let ElementContent::Video { retime, .. } = &mut e.content {
        retime.rate.numerator = 2;
    }
    let before = p.clone();
    validate_project_scene(&p).unwrap();
    assert_eq!(p, before, "validation must not quantize stored times");
    first(&mut p).timeline_range.duration = t(1002, 30000);
    rejected(&p, "disagree with retime");
    p = before;
    first(&mut p).source_span = ElementSourceSpan::LegacySourceRange {
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
    e.source_span = ElementSourceSpan::ClassicTrimmed {
        trim_start: t(0, 1),
        trim_end: t(0, 1),
        source_duration: Some(t(120001, 120000)),
    };
    if let ElementContent::Video { retime, .. } = &mut e.content {
        retime.rate.numerator = 2;
        retime.mapping = RetimeMapping::ClassicRoundedTicks { timebase: 120000 };
    }
    // Classic's 120000 timebase preserves this inverse-rounded rate edit. Its
    // last visible integer local tick (60000) maps to source tick 120000, still
    // inside the source; the request duration itself is not a bounded read span.
    let before = p.clone();
    validate_project_scene(&p).unwrap();
    assert_eq!(p, before);
    let e = &p.sequences[0].tracks[0].elements[0];
    let request = resolve_element_source_request_scene(e, Some(&p.assets[0])).unwrap();
    assert_eq!(
        duration(&request.duration).unwrap(),
        RationalTime::new(120002, 120000).unwrap()
    );
    assert_eq!(p.assets[0].duration, Some(t(120001, 120000)));
    let mut wrong_asset = p.assets[0].clone();
    wrong_asset.id = "other-resource".into();
    assert!(resolve_element_source_request_scene(e, Some(&wrong_asset)).is_err());
    assert!(resolve_element_source_request_scene(e, None).is_err());
    first(&mut p).timeline_range.duration = t(60002, 120000);
    rejected(&p, "both declared rounding directions");
    p = before;
    first(&mut p).source_span = ElementSourceSpan::LegacySourceRange {
        range: span(t(0, 1), t(120002, 120000)),
    };
    if let ElementContent::Video { retime, .. } = &mut first(&mut p).content {
        retime.mapping = RetimeMapping::ExactRational;
    }
    rejected(&p, "exceeds resource duration");
}

#[test]
fn gain_units_do_not_silently_reinterpret_old_linear_values() {
    assert_eq!(
        gain_amplitude(&Gain::LinearLegacy { value: 0.0 }).unwrap(),
        0.0
    );
    assert_eq!(
        gain_amplitude(&Gain::LinearLegacy { value: 4.0 }).unwrap(),
        4.0
    );
    assert_eq!(
        gain_amplitude(&Gain::Decibels { value: 20.0 }).unwrap(),
        10.0
    );
    assert!((gain_amplitude(&Gain::Decibels { value: -60.0 }).unwrap() - 0.001).abs() < 1e-12);
    let mut p = project();
    first(&mut p).channels.push(channel(
        ChannelTarget::Element {
            element_id: "video-element".into(),
        },
        ScalarParameter::GainLinearLegacy,
        vec![key("key", t(0, 1), 1.0)],
    ));
    rejected(&p, "unit or type");
    if let ElementContent::Video { audio, .. } = &mut first(&mut p).content {
        audio.gain = Gain::LinearLegacy { value: 1.0 };
    }
    validate_project_scene(&p).unwrap();
}

#[test]
fn effect_reference_and_parameter_are_bound_to_the_owning_element() {
    let mut p = project();
    first(&mut p)
        .visual
        .as_mut()
        .unwrap()
        .effects
        .push(SceneEffect {
            id: "blur".into(),
            enabled: true,
            effect: EffectKind::ClassicBlur { intensity: 200.0 },
        });
    first(&mut p).channels.push(channel(
        ChannelTarget::Effect {
            element_id: "video-element".into(),
            effect_id: "blur".into(),
        },
        ScalarParameter::ClassicBlurIntensity,
        vec![key("blur-key", t(1, 2), 200.0)],
    ));
    validate_project_scene(&p).unwrap();
    if let AnimationChannel::Scalar { binding, .. } = &mut first(&mut p).channels[0] {
        *binding = ScalarBinding::Property {
            parameter: ScalarParameter::LegacyBlurRadiusPx,
        };
    }
    rejected(&p, "unit or type");
    if let AnimationChannel::Scalar { target, .. } = &mut first(&mut p).channels[0] {
        *target = ChannelTarget::Effect {
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
    a.tangent_mode = TangentMode::Flat;
    a.right_handle = Some(CurveHandle {
        dt: t(1, 3),
        dv: 0.275,
    });
    let mut b = key("b", t(3, 2), 1.0);
    b.left_handle = Some(CurveHandle {
        dt: t(-1, 3),
        dv: 0.275,
    });
    first(&mut p).channels.push(channel(
        ChannelTarget::Element {
            element_id: "video-element".into(),
        },
        ScalarParameter::ScaleX,
        vec![a, b],
    ));
    validate_project_scene(&p).unwrap();
    if let AnimationChannel::Scalar { keys, .. } = &mut first(&mut p).channels[0] {
        keys[1].time = t(1000, 2000);
    }
    rejected(&p, "strictly ordered");
    if let AnimationChannel::Scalar { keys, .. } = &mut first(&mut p).channels[0] {
        keys[1].time = t(3, 2);
        keys[0].right_handle.as_mut().unwrap().dt = t(-1, 3);
    }
    rejected(&p, "wrong direction");
}

#[test]
fn track_channels_use_sequence_time_and_cannot_hide_in_element_local_storage() {
    let mut p = project();
    p.sequences[0].tracks[0].channels.push(channel(
        ChannelTarget::Track {
            track_id: "main".into(),
        },
        ScalarParameter::Opacity,
        vec![key("track-key", t(9, 4), 0.5)],
    ));
    // Local clip duration is 2s, but scene duration is 2.5s.
    validate_project_scene(&p).unwrap();
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
        .push(track("audio-track", SceneTrackKind::Audio, vec![video()]));
    p.sequences[0].tracks[1].elements[0].id = "wrong-kind".into();
    rejected(&p, "incompatible");
}

#[test]
fn audio_presence_preserves_unknown_history_without_treating_absence_as_a_stream() {
    let mut p = project();
    p.assets[0].metadata.audio = AudioPresence::Absent {};
    if let ElementContent::Video {
        source_audio_enabled,
        ..
    } = &mut first(&mut p).content
    {
        *source_audio_enabled = true;
    }
    // Classic's default preference is not an assertion that probing found audio.
    validate_project_scene(&p).unwrap();
    let mut sound = video();
    sound.id = "sound".into();
    sound.visual = None;
    sound.content = ElementContent::Audio {
        asset_id: "media".into(),
        audio: audio(),
        retime: retime(1, 1),
    };
    p.sequences[0]
        .tracks
        .push(track("audio-track", SceneTrackKind::Audio, vec![sound]));
    rejected(&p, "known absent audio");
    // This is structural preservation; an actual detach/restore must still probe.
    p.assets[0].metadata.audio = AudioPresence::Unknown {};
    validate_project_scene(&p).unwrap();
    p.assets[0].metadata.audio = AudioPresence::Present { metadata: None };
    validate_project_scene(&p).unwrap();
    p.assets[0].metadata.audio = AudioPresence::Present {
        metadata: Some(AudioMetadata {
            sample_rate: 48000,
            channels: 2,
            channel_layout: None,
        }),
    };
    validate_project_scene(&p).unwrap();
}

#[test]
fn pinned_caption_references_locked_cas_font_and_bookmarks_keep_their_owning_scene() {
    let mut p = project();
    p.sequences[0].captions.push(Caption {
        id: "caption".into(),
        track_id: "caption-track".into(),
        timeline_range: span(t(1001, 30000), t(1, 1)),
        text: "中文 caption".into(),
        source_anchor: Some(opencut_contracts::CaptionSourceAnchor {
            asset_id: "media".into(),
            source_range: span(t(1, 1), t(1, 1)),
        }),
        style: SceneCaptionStyle {
            font: FontBinding::Locked {
                font_id: "font".into(),
            },
            font_size: FontSize::CanvasPixels { value: 24.0 },
            color: rgba(1.0, 1.0, 1.0),
            background_color: None,
            max_width_px: Some(120),
            line_height_px: Some(28.0),
            text_align: SceneTextAlign::Center,
            layout: CaptionLayoutProfile::PinnedCaptionV4,
        },
    });
    p.sequences[0]
        .tracks
        .push(track("caption-track", SceneTrackKind::Caption, vec![]));
    p.sequences[0].bookmarks.push(SceneBookmark {
        id: "bookmark".into(),
        sequence_id: "scene".into(),
        time: t(1001, 30000),
        duration: Some(t(0, 1)),
        note: Some("note".into()),
        color: Some("#abcdef".into()),
    });
    validate_project_scene(&p).unwrap();
    p.sequences[0].captions[0].track_id = "main".into();
    rejected(&p, "owning caption track");
    p.sequences[0].captions[0].track_id = "caption-track".into();
    p.assets[0].metadata.audio = AudioPresence::Absent {};
    rejected(&p, "known absent audio");
    p.sequences[0].captions[0].style.layout = CaptionLayoutProfile::LegacyCaptionV2;
    validate_project_scene(&p).unwrap();
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
    p.assets[0].metadata.audio = AudioPresence::Unknown {};
    p.sequences[0].captions[0].style.layout = CaptionLayoutProfile::PinnedCaptionV4;
    validate_project_scene(&p).unwrap();
    p.sequences[0].captions[0].style.font = FontBinding::Locked {
        font_id: "missing".into(),
    };
    rejected(&p, "font reference");
    p.sequences[0].captions[0].style.font = FontBinding::UnresolvedFamily {
        family: "Legacy".into(),
        policy: FontResolutionPolicy::Strict {},
    };
    rejected(&p, "Pinned caption");
    p.sequences[0].captions[0].style.layout = CaptionLayoutProfile::LegacyCaptionV2;
    validate_project_scene(&p).unwrap();
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
    p.assets[0].metadata.audio = AudioPresence::Absent {};
    p.sequences[0].tracks[0].kind = SceneTrackKind::Graphic;
    for e in &mut p.sequences[0].tracks[0].elements {
        e.content = ElementContent::Image {
            asset_id: "media".into(),
        };
        e.visual.as_mut().unwrap().profile = VisualProfile::LegacyCompositionV3;
    }
    p.sequences[0].main_track_id = "empty-main".into();
    p.sequences[0]
        .tracks
        .push(track("empty-main", SceneTrackKind::Video, vec![]));
    validate_project_scene(&p).unwrap();
    first(&mut p).visual.as_mut().unwrap().profile = VisualProfile::ClassicCanvasV31;
    rejected(&p, "cannot overlap");
}

#[test]
fn freeform_handles_and_stable_mask_channels_are_validated_without_flattening_geometry() {
    let mut p = project();
    let point = |id: &str, x, y| FreeformPoint {
        id: id.into(),
        x,
        y,
        in_x: x - 0.1,
        in_y: y,
        out_x: x + 0.1,
        out_y: y,
    };
    first(&mut p).visual.as_mut().unwrap().masks.push(Mask {
        id: "path-mask".into(),
        enabled: true,
        inverted: false,
        shape: LayerMaskShape::Freeform {
            path: FreeformMask {
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
        feather: MaskFeather::ClassicPixels { value: 3.0 },
        stroke: Stroke {
            color: rgba(1.0, 0.0, 0.0),
            width_px: 2.0,
            align: StrokeAlign::Outside,
        },
    });
    first(&mut p).channels.push(channel(
        ChannelTarget::Mask {
            element_id: "video-element".into(),
            mask_id: "path-mask".into(),
        },
        ScalarParameter::MaskFeatherClassicPixels,
        vec![key("feather-key", t(1, 2), 4.0)],
    ));
    let before = p.clone();
    validate_project_scene(&p).unwrap();
    assert_eq!(p, before);
    if let AnimationChannel::Scalar { binding, .. } = &mut first(&mut p).channels[0] {
        *binding = ScalarBinding::Property {
            parameter: ScalarParameter::MaskWidth,
        };
    }
    rejected(&p, "unit or type");
    p = before;
    if let LayerMaskShape::Freeform { path } =
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
    p.fonts[0].style = FontStyle::Italic;
    let mut title = video();
    title.id = "title".into();
    title.name = "Title".into();
    title.source_span = ElementSourceSpan::ClassicTrimmed {
        trim_start: t(1, 1),
        trim_end: t(1, 1),
        source_duration: Some(t(4, 1)),
    };
    title.content = ElementContent::Text {
        text: Text {
            content: "中文\nTitle".into(),
            font: FontBinding::Locked {
                font_id: "font".into(),
            },
            font_weight: FontWeight::Bold,
            font_style: FontStyle::Italic,
            decoration: TextDecoration::Underline,
            align: SceneTextAlign::Center,
            font_size: FontSize::ClassicHeight90 { value: 15.0 },
            color: rgba(1.0, 1.0, 1.0),
            letter_spacing_px: 1.0,
            line_height_multiplier: 1.25,
            background: TextBackground {
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
        ChannelTarget::Element {
            element_id: "title".into(),
        },
        ScalarParameter::FontSizeClassicHeight90,
        vec![key("font-key", t(0, 1), 15.0)],
    ));
    p.sequences[0]
        .tracks
        .push(track("text-track", SceneTrackKind::Text, vec![title]));
    let mut shape = video();
    shape.id = "polygon".into();
    shape.name = "Polygon".into();
    shape.source_span = ElementSourceSpan::ClassicTrimmed {
        trim_start: t(0, 1),
        trim_end: t(0, 1),
        source_duration: None,
    };
    shape.content = ElementContent::Graphic {
        graphic: Graphic {
            geometry: GraphicGeometry::Polygon {
                sides: 4.5,
                corner_radius_percent: 25.0,
            },
            source_size: CanvasSize {
                width: 200,
                height: 100,
            },
            fill: rgba(1.0, 0.0, 0.0),
            stroke: Stroke {
                color: rgba(0.0, 0.0, 1.0),
                width_px: 4.0,
                align: StrokeAlign::Inside,
            },
        },
    };
    p.sequences[0]
        .tracks
        .push(track("graphic-track", SceneTrackKind::Graphic, vec![shape]));
    validate_project_scene(&p).unwrap();
    p.fonts[0].weight = 400;
    rejected(&p, "does not match requested weight and style");
    p.fonts[0].weight = 700;
    p.fonts[0].style = FontStyle::Normal;
    rejected(&p, "does not match requested weight and style");
    p.fonts[0].style = FontStyle::Italic;
    if let AnimationChannel::Scalar { binding, .. } =
        &mut p.sequences[0].tracks[1].elements[0].channels[0]
    {
        *binding = ScalarBinding::Property {
            parameter: ScalarParameter::FontSizeCanvasPixels,
        };
    }
    rejected(&p, "unit or type");
}

#[test]
fn custom_canvas_memory_survives_native_first_visual_canvas_override() {
    let mut value = project();
    let SettingsProfile::ClassicGlobal { settings } = &mut value.settings else {
        panic!()
    };
    settings.canvas_size_mode = CanvasSizeMode::Custom;
    settings.last_custom_canvas_size = Some(CanvasSize {
        width: 720,
        height: 1280,
    });
    settings.canvas_size = CanvasSize {
        width: 1920,
        height: 1080,
    };
    validate_project_scene(&value).unwrap();
    let SettingsProfile::ClassicGlobal { settings } = &mut value.settings else {
        panic!()
    };
    settings.last_custom_canvas_size = Some(CanvasSize {
        width: 0,
        height: 1280,
    });
    assert!(validate_project_scene(&value).is_err());
}

#[test]
fn classic_open_freeform_drafts_preserve_empty_and_single_point_geometry() {
    for enabled in [true, false] {
        for count in 0..=2 {
            let mut p = project();
            first(&mut p).visual.as_mut().unwrap().masks.push(Mask {
                id: "draft-mask".into(),
                enabled,
                inverted: true,
                shape: LayerMaskShape::Freeform {
                    path: FreeformMask {
                        points: (0..count)
                            .map(|i| FreeformPoint {
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
                feather: MaskFeather::ClassicPixels { value: 3. },
                stroke: Stroke {
                    color: rgba(1., 0., 0.),
                    width_px: 2.,
                    align: StrokeAlign::Center,
                },
            });
            let before = p.clone();
            validate_project_scene(&p).unwrap();
            assert_eq!(before, p);
            let LayerMaskShape::Freeform { path } =
                &mut first(&mut p).visual.as_mut().unwrap().masks[0].shape
            else {
                panic!()
            };
            path.closed = true;
            rejected(&p, "enough path points");
            p = before;
            if count > 0 {
                let LayerMaskShape::Freeform { path } =
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
