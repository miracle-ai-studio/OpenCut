use super::super::*;
use super::fixture::channel;
use super::fixture::*;

#[test]
fn classic_video_rounds_the_complete_source_point_without_rounding_audio_or_legacy() {
    let mut s = snapshot();
    let e = &mut s.project.sequences[0].tracks[0].elements[0];
    e.timeline_range.start = t(0, 1);
    // Quarter-tick trim + quarter-tick local = half tick, rounded to one.
    e.source_span = ElementSourceSpanV4::LegacySourceRange {
        range: range(t(1, 480000), t(2, 1)),
    };
    e.visual.as_mut().unwrap().profile = VisualProfileV4::ClassicCanvasV31;
    let graph =
        compile_frame_v4(&s, "scene", &t(1, 480000), CompositionScope::AudioVisual).unwrap();
    let SampledContentV4::Video { source, .. } = &graph.layers[0].content else {
        panic!()
    };
    assert_eq!(source.source_time, t(1, 120000));
    assert_eq!(
        source.point_profile,
        SourcePointProfileV4::ClassicVideoRoundedTicks
    );
    assert_eq!(source.nominal_request.start, t(1, 480000));
    assert_eq!(source.requested_read_span.start, t(1, 480000));
    assert_eq!(
        graph.audio[0].source.point_profile,
        SourcePointProfileV4::ExactRational
    );
    assert_eq!(
        time(&graph.audio[0].source.source_time)
            .unwrap()
            .cmp_exact(time(&t(1, 240000)).unwrap()),
        Ordering::Equal
    );

    // Immediately below the half tick is zero, not an independently rounded
    // origin or a conversion through presentation-frame seconds.
    let graph = compile_frame_v4(&s, "scene", &t(1, 600000), CompositionScope::Visual).unwrap();
    let SampledContentV4::Video { source, .. } = &graph.layers[0].content else {
        panic!()
    };
    assert_eq!(source.source_time, t(0, 120000));
    assert_eq!(source.sample, SourceReadStateV4::OutsideRequestedWindow);

    s.project.sequences[0].tracks[0].elements[0]
        .visual
        .as_mut()
        .unwrap()
        .profile = VisualProfileV4::LegacyCompositionV3;
    let graph = compile_frame_v4(&s, "scene", &t(1, 480000), CompositionScope::Visual).unwrap();
    let SampledContentV4::Video { source, .. } = &graph.layers[0].content else {
        panic!()
    };
    assert_eq!(source.point_profile, SourcePointProfileV4::ExactRational);
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
            SourceReadStateV4::InBounds,
        ),
        (
            (1, 1000),
            (1, 100),
            t(601, 120000),
            t(61, 120000),
            t(1, 500),
            SourceReadStateV4::OutsideRequestedWindow,
        ),
    ] {
        let mut s = snapshot();
        s.project.assets[0].duration = Some(t(30, 1));
        let e = &mut s.project.sequences[0].tracks[0].elements[0];
        e.timeline_range.start = t(0, 1);
        e.source_span = ElementSourceSpanV4::LegacySourceRange {
            range: range(t(1, 120000), request_duration.clone()),
        };
        e.visual.as_mut().unwrap().profile = VisualProfileV4::ClassicCanvasV31;
        if let ElementContentV4::Video { retime, .. } = &mut e.content {
            retime.rate = RationalRateV4 {
                numerator: raw.0,
                denominator: raw.1,
            };
        }
        let graph = compile_frame_v4(&s, "scene", &t(1, 2), CompositionScope::AudioVisual).unwrap();
        let SampledContentV4::Video { source, .. } = &graph.layers[0].content else {
            panic!()
        };
        assert_eq!(
            source.rate,
            RationalRateV4 {
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
            RationalRateV4 {
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
        assert_eq!(audio.point_profile, SourcePointProfileV4::ExactRational);
    }
}

#[test]
fn actual_element_and_track_clocks_drive_equal_endpoint_bump_and_db_projection() {
    let mut s = snapshot();
    let e = &mut s.project.sequences[0].tracks[0].elements[0];
    let mut a = key("opacity-a", t(0, 1), 0.0);
    a.right_handle = Some(CurveHandleV4 {
        dt: t(2, 3),
        dv: 2.0,
    });
    let mut b = key("opacity-b", t(2, 1), 0.0);
    b.left_handle = Some(CurveHandleV4 {
        dt: t(-2, 3),
        dv: 2.0,
    });
    e.channels.push(channel(
        "opacity",
        ChannelTargetV4::Element {
            element_id: "video".into(),
        },
        ScalarParameterV4::Opacity,
        vec![a, b],
    ));
    if let ElementContentV4::Video { audio, .. } = &mut e.content {
        audio.gain = GainV4::Decibels { value: 50.0 };
    }
    s.project.sequences[0].tracks[0].channels.push(channel(
        "track-db",
        ChannelTargetV4::Track {
            track_id: "main".into(),
        },
        ScalarParameterV4::GainDecibels,
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
        GainV4::Decibels { value: 20.0 }
    );
    assert_eq!(
        graph.audio[0].track_gain.physical,
        GainV4::Decibels { value: -60.0 }
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
        assert!(graph.coverage.iter().any(|e| e.source_id == id
            && matches!(e.disposition, CoverageDispositionV4::Compiled { .. })));
    }
}

#[test]
fn inverse_rounding_request_and_bounded_reads_are_distinct_and_unknown_extent_stays_unknown() {
    let mut s = snapshot();
    s.project.sequences[0].tracks.pop();
    s.project.assets[0].duration = Some(t(120001, 120000));
    let e = &mut s.project.sequences[0].tracks[0].elements[0];
    e.timeline_range = range(t(0, 1), t(60001, 120000));
    e.source_span = ElementSourceSpanV4::ClassicTrimmed {
        trim_start: t(0, 1),
        trim_end: t(0, 1),
        source_duration: Some(t(120001, 120000)),
    };
    if let ElementContentV4::Video { retime, .. } = &mut e.content {
        retime.rate.numerator = 2;
        retime.mapping = RetimeMappingV4::ClassicRoundedTicks { timebase: 120000 };
    }
    let graph = compile_frame_v4(&s, "scene", &t(60000, 120000), CompositionScope::Visual).unwrap();
    let SampledContentV4::Video { source, .. } = &graph.layers[0].content else {
        panic!()
    };
    assert_eq!(
        time(&source.nominal_request.duration)
            .unwrap()
            .cmp_exact(time(&t(120002, 120000)).unwrap()),
        Ordering::Equal
    );
    let BoundedReadWindowV4::Known {
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
    assert_eq!(source.sample, SourceReadStateV4::InBounds);
    assert!(graph.audio.is_empty());
    s.project.assets[0].duration = None;
    let graph = compile_frame_v4(&s, "scene", &t(60000, 120000), CompositionScope::Visual).unwrap();
    let SampledContentV4::Video { source, .. } = &graph.layers[0].content else {
        panic!()
    };
    assert_eq!(
        source.read_window,
        BoundedReadWindowV4::UnknownResourceExtent
    );
    assert_eq!(source.sample, SourceReadStateV4::NeedsResourceProbe);
    assert!(
        required_capabilities_v4(&graph)
            .iter()
            .any(|r| r.feature == NativeFeatureV4::ResourceProbe)
    );
}

#[test]
fn exclusions_cover_disabled_out_of_frame_and_visual_only_audio_without_partial_graphs() {
    let mut s = snapshot();
    s.project.sequences[0].tracks[1].enabled = false;
    let e = &mut s.project.sequences[0].tracks[0].elements[0];
    e.channels.push(channel(
        "gain-c",
        ChannelTargetV4::Element {
            element_id: "video".into(),
        },
        ScalarParameterV4::GainDecibels,
        vec![key("gain-k", t(0, 1), 10.0)],
    ));
    let graph = compile_frame_v4(&s, "scene", &t(3, 2), CompositionScope::Visual).unwrap();
    excluded(&graph, "shape", ExclusionV4::DisabledTrack);
    excluded(&graph, "graphics", ExclusionV4::DisabledTrack);
    excluded(&graph, "gain-c", ExclusionV4::VisualScope);
    excluded(&graph, "gain-k", ExclusionV4::VisualScope);
    assert!(graph.audio.is_empty());
    let graph = compile_frame_v4(&s, "scene", &t(3, 1), CompositionScope::AudioVisual).unwrap();
    excluded(&graph, "video", ExclusionV4::OutsideFrame);
    excluded(&graph, "gain-c", ExclusionV4::OutsideFrame);
    assert!(graph.layers.is_empty());
    s.revision.clear();
    assert_eq!(
        compile_frame_v4(&s, "scene", &t(1, 1), CompositionScope::Visual)
            .unwrap_err()
            .code,
        "INVALID_PROJECT"
    );
}

#[test]
fn explicit_legacy_format_scope_and_source_audio_probe_knowledge_are_preserved() {
    let mut s = snapshot();
    s.project.settings = SettingsProfileV4::LegacyPerScene {
        formats: vec![LegacySequenceFormatV4 {
            sequence_id: "scene".into(),
            fps: FrameRate {
                numerator: 24,
                denominator: 1,
            },
            canvas_size: CanvasSizeV4 {
                width: 320,
                height: 180,
            },
            background: BackgroundV4::MainTrackBlur { intensity: 25.0 },
        }],
    };
    s.project.assets[0].metadata.audio = AudioPresenceV4::Unknown {};
    let graph = compile(&s);
    assert_eq!(graph.format.profile, FormatProfileV4::LegacyPerScene);
    assert_eq!(
        graph.format.canvas_size,
        CanvasSizeV4 {
            width: 320,
            height: 180
        }
    );
    assert_eq!(graph.audio[0].presence, AudioPresenceV4::Unknown {});
    let SampledContentV4::Video { source_audio, .. } = &graph.layers[0].content else {
        panic!()
    };
    assert_eq!(*source_audio, SourceAudioDecisionV4::RequiresProbe);
    let CompiledBackgroundV4::MainTrackBlur { source_layers, .. } = &graph.background else {
        panic!()
    };
    assert_eq!(source_layers, &vec!["video".to_string()]);
    s.project.assets[0].metadata.audio = AudioPresenceV4::Absent {};
    let graph = compile(&s);
    assert!(graph.audio.is_empty());
    let SampledContentV4::Video { source_audio, .. } = &graph.layers[0].content else {
        panic!()
    };
    assert_eq!(*source_audio, SourceAudioDecisionV4::KnownAbsent);
}

#[test]
fn excluded_audio_never_evaluates_overflowing_gain_or_claims_a_phantom_execution_node() {
    let mut s = snapshot();
    let extreme = |id: &str, target| {
        channel(
            id,
            target,
            ScalarParameterV4::GainDecibels,
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
        ChannelTargetV4::Track {
            track_id: "main".into(),
        },
    ));
    media_track.elements[0].channels.push(extreme(
        "element-overflow",
        ChannelTargetV4::Element {
            element_id: "video".into(),
        },
    ));
    let graph = compile(&s);
    assert!(graph.audio.is_empty());
    excluded(&graph, "track-overflow", ExclusionV4::MutedAudio);
    excluded(&graph, "element-overflow", ExclusionV4::MutedAudio);
    s.project.sequences[0].tracks[0].muted = false;
    if let ElementContentV4::Video {
        source_audio_enabled,
        ..
    } = &mut s.project.sequences[0].tracks[0].elements[0].content
    {
        *source_audio_enabled = false;
    }
    let graph = compile(&s);
    excluded(&graph, "track-overflow", ExclusionV4::SourceAudioDisabled);
    excluded(&graph, "element-overflow", ExclusionV4::SourceAudioDisabled);
    // Standalone muted audio has no visual execution node and must be excluded
    // recursively, including its CAS binding when no other node uses it.
    let mut audio = s.project.sequences[0].tracks[0].elements.remove(0);
    audio.id = "sound".into();
    audio.visual = None;
    audio.channels.clear();
    audio.content = ElementContentV4::Audio {
        asset_id: "asset".into(),
        audio: AudioStyleV4 {
            stream: None,
            gain: GainV4::Decibels { value: 0.0 },
            muted: true,
        },
        retime: RetimeV4 {
            rate: RationalRateV4 {
                numerator: 1,
                denominator: 1,
            },
            maintain_pitch: true,
            mapping: RetimeMappingV4::ExactRational,
        },
    };
    s.project.sequences[0].tracks[0].channels.clear();
    s.project.sequences[0]
        .tracks
        .push(track("sound-track", TrackKindV4::Audio, vec![audio]));
    let graph = compile(&s);
    excluded(&graph, "sound", ExclusionV4::MutedAudio);
    excluded(&graph, "asset", ExclusionV4::UnusedResource);
    assert!(graph.resources.is_empty());
}
