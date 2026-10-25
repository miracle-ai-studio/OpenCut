use super::*;

fn t(n: i64, d: u32) -> Time {
    Time {
        ticks: n.to_string(),
        timebase: d,
    }
}
fn graph() -> SceneComposition {
    SceneComposition {
        project_id: "project".into(),
        revision: "fixed-r1".into(),
        sequence_id: "scene".into(),
        scope: CompositionScope::Visual,
        time: t(1, 7),
        format: CompiledFormat {
            fps: FrameRate {
                numerator: 30,
                denominator: 1,
            },
            canvas_size: CanvasSize {
                width: 64,
                height: 48,
            },
            profile: FormatProfile::ClassicGlobal,
        },
        tracks: vec![],
        layers: vec![CompiledLayer {
            element_id: "element".into(),
            track_id: "main".into(),
            track_order: 0,
            element_order: 0,
            local_time: t(1, 7),
            content: SampledContent::Image {
                asset_id: "asset".into(),
            },
            visual: Some(SampledVisual {
                profile: VisualProfile::ClassicCanvasV31,
                transform: Transform {
                    position_x_px: 3.0,
                    position_y_px: -4.0,
                    scale_x: -2.0,
                    scale_y: 0.5,
                    rotation_degrees: 90.0,
                },
                element_opacity: 1.25,
                track_opacity: 1.0,
                effective_opacity: 1.25,
                opacity_policy: LayerOpacityPolicy::MultiplyTrack,
                blend_mode: BlendMode::Normal,
                effects: vec![],
                masks: vec![],
            }),
        }],
        audio: vec![],
        transitions: vec![],
        fonts: vec![],
        coverage: vec![],
        background: CompiledBackground::Color {
            color: Color {
                encoding: ColorEncoding::Srgb,
                red: 0.2,
                green: 0.4,
                blue: 0.6,
                alpha: 0.25,
            },
        },
        resources: vec![CasResource {
            asset_id: "asset".into(),
            version: AssetVersion {
                id: format!("av_{}", "a".repeat(64)),
                content_hash: "a".repeat(64),
                size: 16,
            },
            kind: AssetKind::Image,
            duration: None,
            metadata: ProjectMediaMetadata {
                mime_type: None,
                intrinsic_size: Some(CanvasSize {
                    width: 1,
                    height: 1,
                }),
                frame_rate: None,
                rotation_degrees: None,
                audio: AudioPresence::Absent {},
            },
            license: None,
        }],
    }
}
fn image(id: &str) -> DecodedVisualSource {
    DecodedVisualSource::ReadyImage {
        element_id: id.into(),
        width: 320,
        height: 90,
    }
}
fn video_graph() -> SceneComposition {
    let mut g = graph();
    let range = TimeRange {
        start: t(1, 1),
        duration: t(2, 1),
    };
    g.layers[0].content = SampledContent::Video {
        asset_id: "asset".into(),
        stream: None,
        source_audio: SourceAudioDecision::ScopeExcluded,
        source: SourceClock {
            timeline_local: t(1, 7),
            source_time: t(5, 4),
            point_profile: SourcePointProfile::ClassicVideoRoundedTicks,
            rate: RationalRate {
                numerator: 3,
                denominator: 2,
            },
            mapping: RetimeMapping::ClassicRoundedTicks { timebase: 120000 },
            nominal_request: range.clone(),
            requested_read_span: range.clone(),
            declared_resize_extent: Some(t(4, 1)),
            read_window: BoundedReadWindow::Known {
                resource_extent: t(4, 1),
                range: Some(range),
            },
            sample: SourceReadState::InBounds,
            eof_policy: EofPolicy::RequiresBackendProfile,
        },
    };
    g.resources[0].kind = AssetKind::Video;
    g
}
fn video(timestamp: f64, duration: f64) -> DecodedVisualSource {
    DecodedVisualSource::ReadyVideo {
        element_id: "element".into(),
        width: 320,
        height: 90,
        requested_time: t(5, 4),
        selected_video_stream: None,
        actual_timestamp_seconds: timestamp,
        frame_duration_seconds: duration,
        eof_held: false,
    }
}
#[test]
fn actual_contain_flip_center_raw_opacity_and_alpha_have_independent_expected_values() {
    for (format, profile) in [
        (
            FormatProfile::ClassicGlobal,
            VisualProfile::ClassicCanvasV31,
        ),
        (
            FormatProfile::LegacyPerScene,
            VisualProfile::LegacyCompositionV3,
        ),
    ] {
        let mut graph = graph();
        graph.format.profile = format;
        graph.layers[0].visual.as_mut().unwrap().profile = profile;
        let prepared = prepare_visual_frame_scene(&graph).unwrap();
        let bound = bind_visual_sources_scene(&prepared, &[image("element")]).unwrap();
        assert_eq!(bound.identity.time, t(1, 7));
        assert_eq!(bound.clear, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(bound.background, [0.2, 0.4, 0.6, 0.25]);
        let q = bound.layers[0].quad.as_ref().unwrap();
        assert_eq!(
            (q.center_x, q.center_y, q.width, q.height),
            (35.0, 20.0, 128.0, 9.0)
        );
        assert_eq!(q.rotation_radians, std::f32::consts::FRAC_PI_2);
        assert!(q.flip_x);
        assert!(!q.flip_y);
        assert_eq!(q.opacity, 1.25);
        assert_eq!(
            prepared.layers()[0].visual.as_ref().unwrap().profile,
            profile
        );
    }
}
#[test]
fn complete_unique_decode_set_preserves_layer_order_and_zero_transform() {
    let mut g = graph();
    let mut second = g.layers[0].clone();
    second.element_id = "second".into();
    second.element_order = 1;
    second.visual.as_mut().unwrap().transform.scale_x = 0.0;
    g.layers.push(second);
    let prepared = prepare_visual_frame_scene(&g).unwrap();
    let bound = bind_visual_sources_scene(&prepared, &[image("second"), image("element")]).unwrap();
    assert_eq!(
        bound
            .layers
            .iter()
            .map(|l| l.element_id.as_str())
            .collect::<Vec<_>>(),
        ["element", "second"]
    );
    assert!(bound.layers[1].quad.is_none());
    for bad in [
        vec![image("element")],
        vec![image("element"), image("element")],
        vec![image("element"), image("unknown")],
    ] {
        assert!(bind_visual_sources_scene(&prepared, &bad).is_err());
    }
}
#[test]
fn video_fixed_point_and_half_open_observation_interval_do_not_hold_gap_frames() {
    let prepared = prepare_visual_frame_scene(&video_graph()).unwrap();
    assert!(bind_visual_sources_scene(&prepared, &[video(1.2, 0.1)]).is_ok());
    for bad in [
        video(1.0, 0.25),
        video(1.26, 0.1),
        video(1.0, f64::NAN),
        video(f64::MAX, f64::MAX),
    ] {
        assert!(bind_visual_sources_scene(&prepared, &[bad]).is_err());
    }
    let mut changed = video(1.2, 0.1);
    if let DecodedVisualSource::ReadyVideo { requested_time, .. } = &mut changed {
        *requested_time = t(3, 2);
    }
    assert!(bind_visual_sources_scene(&prepared, &[changed]).is_err());
    let gap = DecodedVisualSource::NoFrame {
        element_id: "element".into(),
        requested_time: t(5, 4),
        reason: VisualNoFrameReason::TimestampGap,
    };
    assert!(
        bind_visual_sources_scene(&prepared, &[gap]).unwrap().layers[0]
            .quad
            .is_none()
    );
    let outside = DecodedVisualSource::NoFrame {
        element_id: "element".into(),
        requested_time: t(5, 4),
        reason: VisualNoFrameReason::OutsideReadWindow,
    };
    assert!(bind_visual_sources_scene(&prepared, &[outside]).is_err());
    let mut held = video(1.0, 0.1);
    let DecodedVisualSource::ReadyVideo { eof_held, .. } = &mut held else {
        unreachable!()
    };
    *eof_held = true;
    assert!(bind_visual_sources_scene(&prepared, &[held]).is_err());

    let mut retained = video_graph();
    retained.layers[0].visual.as_mut().unwrap().profile = VisualProfile::LegacyCompositionV3;
    let SampledContent::Video { source, .. } = &mut retained.layers[0].content else {
        unreachable!()
    };
    source.point_profile = SourcePointProfile::ExactRational;
    source.source_time = t(17, 14); // 1 + (1/7 * 3/2); not on the Classic tick grid.
    let prepared = prepare_visual_frame_scene(&retained).unwrap();
    let mut observed = video(1.2, 0.1);
    let DecodedVisualSource::ReadyVideo { requested_time, .. } = &mut observed else {
        unreachable!()
    };
    *requested_time = t(17, 14);
    assert_eq!(
        bind_visual_sources_scene(&prepared, &[observed])
            .unwrap()
            .decoded[0]
            .element_id(),
        "element"
    );
    let SampledContent::Video { source, .. } = &mut retained.layers[0].content else {
        unreachable!()
    };
    source.point_profile = SourcePointProfile::ClassicVideoRoundedTicks;
    assert!(prepare_visual_frame_scene(&retained).is_err());
    let SampledContent::Video { source, .. } = &mut retained.layers[0].content else {
        unreachable!()
    };
    source.point_profile = SourcePointProfile::ExactRational;
    source.sample = SourceReadState::EndOfResource;
    source.source_time = t(5, 1);
    source.nominal_request.duration = t(5, 1);
    source.requested_read_span.duration = t(5, 1);
    source.read_window = BoundedReadWindow::Known {
        resource_extent: t(4, 1),
        range: Some(TimeRange {
            start: t(1, 1),
            duration: t(3, 1),
        }),
    };
    let prepared = prepare_visual_frame_scene(&retained).unwrap();
    assert_eq!(
        video_eof_policy_scene(&retained.layers[0]).unwrap(),
        VisualVideoEofPolicy::HoldLastFrame {
            read_window_start_seconds: 1.0,
            read_window_end_seconds: 4.0
        }
    );
    let mut terminal = video(3.8, 0.2);
    let DecodedVisualSource::ReadyVideo {
        requested_time,
        eof_held,
        ..
    } = &mut terminal
    else {
        unreachable!()
    };
    *requested_time = t(5, 1);
    *eof_held = true;
    let bound = bind_visual_sources_scene(&prepared, std::slice::from_ref(&terminal)).unwrap();
    assert!(bound.layers[0].quad.is_some());
    assert_eq!(bound.decoded, [terminal.clone()]);
    for (timestamp, duration, held) in [
        (3.8, 0.2, false),
        (5.0, 0.2, true),
        (0.0, 0.5, true),
        (3.8, f64::NAN, true),
    ] {
        let mut bad = terminal.clone();
        let DecodedVisualSource::ReadyVideo {
            actual_timestamp_seconds,
            frame_duration_seconds,
            eof_held,
            ..
        } = &mut bad
        else {
            unreachable!()
        };
        *actual_timestamp_seconds = timestamp;
        *frame_duration_seconds = duration;
        *eof_held = held;
        assert!(bind_visual_sources_scene(&prepared, &[bad]).is_err());
    }
}
#[test]
fn unsupported_graphs_stream_selection_and_nonfinite_uniforms_fail_before_decode() {
    let mut g = graph();
    g.format.canvas_size.width = 0;
    assert!(prepare_visual_frame_scene(&g).is_err());
    let mut g = graph();
    g.layers[0].visual.as_mut().unwrap().blend_mode = BlendMode::Multiply;
    assert!(prepare_visual_frame_scene(&g).is_err());
    let mut g = graph();
    g.layers[0].visual.as_mut().unwrap().transform.position_x_px = f64::MAX;
    assert!(prepare_visual_frame_scene(&g).is_err());
    let mut g = video_graph();
    if let SampledContent::Video { stream, .. } = &mut g.layers[0].content {
        *stream = Some(0);
    }
    assert!(prepare_visual_frame_scene(&g).is_err());
}
