use super::super::*;
use super::fixture::channel;
use super::fixture::*;

#[test]
fn classic_video_rounds_the_complete_source_point_without_rounding_audio_or_legacy() {
    let mut s = snapshot();
    let e = &mut s.project.sequences[0].tracks[0].elements[0];
    e.timeline_range.start = t(0, 1);
    // Quarter-tick trim + quarter-tick local = half tick, rounded to one.
    e.source_span = ElementSourceSpan::LegacySourceRange {
        range: range(t(1, 480000), t(2, 1)),
    };
    e.visual.as_mut().unwrap().profile = VisualProfile::ClassicCanvasV31;
    let graph =
        compile_frame_scene(&s, "scene", &t(1, 480000), CompositionScope::AudioVisual).unwrap();
    let SampledContent::Video { source, .. } = &graph.layers[0].content else {
        panic!()
    };
    assert_eq!(source.source_time, t(1, 120000));
    assert_eq!(
        source.point_profile,
        SourcePointProfile::ClassicVideoRoundedTicks
    );
    assert_eq!(source.nominal_request.start, t(1, 480000));
    assert_eq!(source.requested_read_span.start, t(1, 480000));
    assert_eq!(
        graph.audio[0].source.point_profile,
        SourcePointProfile::ExactRational
    );
    assert_eq!(
        time(&graph.audio[0].source.source_time)
            .unwrap()
            .cmp_exact(time(&t(1, 240000)).unwrap()),
        Ordering::Equal
    );

    // Immediately below the half tick is zero, not an independently rounded
    // origin or a conversion through presentation-frame seconds.
    let graph = compile_frame_scene(&s, "scene", &t(1, 600000), CompositionScope::Visual).unwrap();
    let SampledContent::Video { source, .. } = &graph.layers[0].content else {
        panic!()
    };
    assert_eq!(source.source_time, t(0, 120000));
    assert_eq!(source.sample, SourceReadState::OutsideRequestedWindow);

    s.project.sequences[0].tracks[0].elements[0]
        .visual
        .as_mut()
        .unwrap()
        .profile = VisualProfile::LegacyCompositionV3;
    let graph = compile_frame_scene(&s, "scene", &t(1, 480000), CompositionScope::Visual).unwrap();
    let SampledContent::Video { source, .. } = &graph.layers[0].content else {
        panic!()
    };
    assert_eq!(source.point_profile, SourcePointProfile::ExactRational);
    assert_eq!(
        time(&source.source_time)
            .unwrap()
            .cmp_exact(time(&t(1, 240000)).unwrap()),
        Ordering::Equal
    );
}

#[test]
fn classic_video_clamps_rate_without_rewriting_authored_requests_or_audio() {
    for (raw, effective, visual_point, audio_point, request_duration, state) in [
        (
            (10, 1),
            (5, 1),
            t(300001, 120000),
            t(600001, 120000),
            t(20, 1),
            SourceReadState::InBounds,
        ),
        (
            (1, 1000),
            (1, 100),
            t(601, 120000),
            t(61, 120000),
            t(1, 500),
            SourceReadState::OutsideRequestedWindow,
        ),
    ] {
        let mut s = snapshot();
        s.project.assets[0].duration = Some(t(30, 1));
        let e = &mut s.project.sequences[0].tracks[0].elements[0];
        e.timeline_range.start = t(0, 1);
        e.source_span = ElementSourceSpan::LegacySourceRange {
            range: range(t(1, 120000), request_duration.clone()),
        };
        e.visual.as_mut().unwrap().profile = VisualProfile::ClassicCanvasV31;
        if let ElementContent::Video { retime, .. } = &mut e.content {
            retime.rate = RationalRate {
                numerator: raw.0,
                denominator: raw.1,
            };
        }
        let graph =
            compile_frame_scene(&s, "scene", &t(1, 2), CompositionScope::AudioVisual).unwrap();
        let SampledContent::Video { source, .. } = &graph.layers[0].content else {
            panic!()
        };
        assert_eq!(
            source.rate,
            RationalRate {
                numerator: effective.0,
                denominator: effective.1
            }
        );
        assert_eq!(source.source_time, visual_point);
        assert_eq!(source.nominal_request.duration, request_duration);
        assert_eq!(source.requested_read_span, source.nominal_request);
        assert_eq!(source.sample, state);
        let audio = &graph.audio[0].source;
        assert_eq!(
            audio.rate,
            RationalRate {
                numerator: raw.0,
                denominator: raw.1
            }
        );
        assert_eq!(
            time(&audio.source_time)
                .unwrap()
                .cmp_exact(time(&audio_point).unwrap()),
            Ordering::Equal
        );
        assert_eq!(audio.point_profile, SourcePointProfile::ExactRational);
    }
}

#[test]
fn actual_element_and_track_clocks_drive_equal_endpoint_bump_and_db_projection() {
    let mut s = snapshot();
    let e = &mut s.project.sequences[0].tracks[0].elements[0];
    let mut a = key("opacity-a", t(0, 1), 0.0);
    a.right_handle = Some(CurveHandle {
        dt: t(2, 3),
        dv: 2.0,
    });
    let mut b = key("opacity-b", t(2, 1), 0.0);
    b.left_handle = Some(CurveHandle {
        dt: t(-2, 3),
        dv: 2.0,
    });
    e.channels.push(channel(
        "opacity",
        ChannelTarget::Element {
            element_id: "video".into(),
        },
        ScalarParameter::Opacity,
        vec![a, b],
    ));
    if let ElementContent::Video { audio, .. } = &mut e.content {
        audio.gain = Gain::Decibels { value: 50.0 };
    }
    s.project.sequences[0].tracks[0].channels.push(channel(
        "track-db",
        ChannelTarget::Track {
            track_id: "main".into(),
        },
        ScalarParameter::GainDecibels,
        vec![
            key("track-k0", t(0, 1), -100.0),
            key("track-k1", t(5, 2), -100.0),
        ],
    ));
    let graph = compile(&s);
    // Local time 1s at global 1.5s -> Bernstein bump 1.5, multiplied by .25.
    close(
        graph.layers[0].visual.as_ref().unwrap().element_opacity,
        1.5,
    );
    close(
        graph.layers[0].visual.as_ref().unwrap().effective_opacity,
        0.375,
    );
    assert_eq!(
        graph.audio[0].element_gain.physical,
        Gain::Decibels { value: 20.0 }
    );
    assert_eq!(
        graph.audio[0].track_gain.physical,
        Gain::Decibels { value: -60.0 }
    );
    close(graph.audio[0].element_gain.amplitude, 10.0);
    close(graph.audio[0].track_gain.amplitude, 0.001);
    close(graph.audio[0].amplitude, 0.01);
    for id in [
        "opacity",
        "opacity-a",
        "opacity-b",
        "track-db",
        "track-k0",
        "track-k1",
    ] {
        assert!(
            graph.coverage.iter().any(|e| e.source_id == id
                && matches!(e.disposition, CoverageDisposition::Compiled { .. }))
        );
    }
}

#[test]
fn inverse_rounding_request_and_bounded_reads_are_distinct_and_unknown_extent_stays_unknown() {
    let mut s = snapshot();
    s.project.sequences[0].tracks.pop();
    s.project.assets[0].duration = Some(t(120001, 120000));
    let e = &mut s.project.sequences[0].tracks[0].elements[0];
    e.timeline_range = range(t(0, 1), t(60001, 120000));
    e.source_span = ElementSourceSpan::ClassicTrimmed {
        trim_start: t(0, 1),
        trim_end: t(0, 1),
        source_duration: Some(t(120001, 120000)),
    };
    if let ElementContent::Video { retime, .. } = &mut e.content {
        retime.rate.numerator = 2;
        retime.mapping = RetimeMapping::ClassicRoundedTicks { timebase: 120000 };
    }
    let graph =
        compile_frame_scene(&s, "scene", &t(60000, 120000), CompositionScope::Visual).unwrap();
    let SampledContent::Video { source, .. } = &graph.layers[0].content else {
        panic!()
    };
    assert_eq!(
        time(&source.nominal_request.duration)
            .unwrap()
            .cmp_exact(time(&t(120002, 120000)).unwrap()),
        Ordering::Equal
    );
    let BoundedReadWindow::Known {
        range: Some(window),
        ..
    } = &source.read_window
    else {
        panic!()
    };
    assert_eq!(
        end(window)
            .unwrap()
            .cmp_exact(time(&t(120001, 120000)).unwrap()),
        Ordering::Equal
    );
    assert_eq!(source.sample, SourceReadState::InBounds);
    assert!(graph.audio.is_empty());
    s.project.assets[0].duration = None;
    let graph =
        compile_frame_scene(&s, "scene", &t(60000, 120000), CompositionScope::Visual).unwrap();
    let SampledContent::Video { source, .. } = &graph.layers[0].content else {
        panic!()
    };
    assert_eq!(source.read_window, BoundedReadWindow::UnknownResourceExtent);
    assert_eq!(source.sample, SourceReadState::NeedsResourceProbe);
    assert!(
        required_capabilities_scene(&graph)
            .iter()
            .any(|r| r.feature == NativeFeature::ResourceProbe)
    );
}

#[test]
fn exclusions_cover_disabled_out_of_frame_and_visual_only_audio_without_partial_graphs() {
    let mut s = snapshot();
    s.project.sequences[0].tracks[1].enabled = false;
    let e = &mut s.project.sequences[0].tracks[0].elements[0];
    e.channels.push(channel(
        "gain-c",
        ChannelTarget::Element {
            element_id: "video".into(),
        },
        ScalarParameter::GainDecibels,
        vec![key("gain-k", t(0, 1), 10.0)],
    ));
    let graph = compile_frame_scene(&s, "scene", &t(3, 2), CompositionScope::Visual).unwrap();
    excluded(&graph, "shape", Exclusion::DisabledTrack);
    excluded(&graph, "graphics", Exclusion::DisabledTrack);
    excluded(&graph, "gain-c", Exclusion::VisualScope);
    excluded(&graph, "gain-k", Exclusion::VisualScope);
    assert!(graph.audio.is_empty());
    let graph = compile_frame_scene(&s, "scene", &t(3, 1), CompositionScope::AudioVisual).unwrap();
    excluded(&graph, "video", Exclusion::OutsideFrame);
    excluded(&graph, "gain-c", Exclusion::OutsideFrame);
    assert!(graph.layers.is_empty());
    s.revision.clear();
    assert_eq!(
        compile_frame_scene(&s, "scene", &t(1, 1), CompositionScope::Visual)
            .unwrap_err()
            .code,
        "INVALID_PROJECT"
    );
}

#[test]
fn explicit_legacy_format_scope_and_source_audio_probe_knowledge_are_preserved() {
    let mut s = snapshot();
    s.project.settings = SettingsProfile::LegacyPerScene {
        formats: vec![LegacySequenceFormat {
            sequence_id: "scene".into(),
            fps: FrameRate {
                numerator: 24,
                denominator: 1,
            },
            canvas_size: CanvasSize {
                width: 320,
                height: 180,
            },
            background: Background::MainTrackBlur { intensity: 25.0 },
        }],
    };
    s.project.assets[0].metadata.audio = AudioPresence::Unknown {};
    let graph = compile(&s);
    assert_eq!(graph.format.profile, FormatProfile::LegacyPerScene);
    assert_eq!(
        graph.format.canvas_size,
        CanvasSize {
            width: 320,
            height: 180
        }
    );
    assert_eq!(graph.audio[0].presence, AudioPresence::Unknown {});
    let SampledContent::Video { source_audio, .. } = &graph.layers[0].content else {
        panic!()
    };
    assert_eq!(*source_audio, SourceAudioDecision::RequiresProbe);
    let CompiledBackground::MainTrackBlur { source_layers, .. } = &graph.background else {
        panic!()
    };
    assert_eq!(source_layers, &vec!["video".to_string()]);
    s.project.assets[0].metadata.audio = AudioPresence::Absent {};
    let graph = compile(&s);
    assert!(graph.audio.is_empty());
    let SampledContent::Video { source_audio, .. } = &graph.layers[0].content else {
        panic!()
    };
    assert_eq!(*source_audio, SourceAudioDecision::KnownAbsent);
}

#[test]
fn excluded_audio_never_evaluates_overflowing_gain_or_claims_a_phantom_execution_node() {
    let mut s = snapshot();
    let extreme = |id: &str, target| {
        channel(
            id,
            target,
            ScalarParameter::GainDecibels,
            vec![
                key(&format!("{id}-a"), t(0, 1), -f64::MAX * 0.4),
                key(&format!("{id}-b"), t(1, u32::MAX), -f64::MAX * 0.2),
            ],
        )
    };
    let media_track = &mut s.project.sequences[0].tracks[0];
    media_track.muted = true;
    media_track.channels.push(extreme(
        "track-overflow",
        ChannelTarget::Track {
            track_id: "main".into(),
        },
    ));
    media_track.elements[0].channels.push(extreme(
        "element-overflow",
        ChannelTarget::Element {
            element_id: "video".into(),
        },
    ));
    let graph = compile(&s);
    assert!(graph.audio.is_empty());
    excluded(&graph, "track-overflow", Exclusion::MutedAudio);
    excluded(&graph, "element-overflow", Exclusion::MutedAudio);
    s.project.sequences[0].tracks[0].muted = false;
    if let ElementContent::Video {
        source_audio_enabled,
        ..
    } = &mut s.project.sequences[0].tracks[0].elements[0].content
    {
        *source_audio_enabled = false;
    }
    let graph = compile(&s);
    excluded(&graph, "track-overflow", Exclusion::SourceAudioDisabled);
    excluded(&graph, "element-overflow", Exclusion::SourceAudioDisabled);
    // Standalone muted audio has no visual execution node and must be excluded
    // recursively, including its CAS binding when no other node uses it.
    let mut audio = s.project.sequences[0].tracks[0].elements.remove(0);
    audio.id = "sound".into();
    audio.visual = None;
    audio.channels.clear();
    audio.content = ElementContent::Audio {
        asset_id: "asset".into(),
        audio: AudioStyle {
            stream: None,
            gain: Gain::Decibels { value: 0.0 },
            muted: true,
        },
        retime: Retime {
            rate: RationalRate {
                numerator: 1,
                denominator: 1,
            },
            maintain_pitch: true,
            mapping: RetimeMapping::ExactRational,
        },
    };
    s.project.sequences[0].tracks[0].channels.clear();
    s.project.sequences[0]
        .tracks
        .push(track("sound-track", SceneTrackKind::Audio, vec![audio]));
    let graph = compile(&s);
    excluded(&graph, "sound", Exclusion::MutedAudio);
    excluded(&graph, "asset", Exclusion::UnusedResource);
    assert!(graph.resources.is_empty());
}
