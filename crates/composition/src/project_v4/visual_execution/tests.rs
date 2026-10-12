use super::*;

fn t(n: i64, d: u32) -> Time {
    Time {
        ticks: n.to_string(),
        timebase: d,
    }
}
fn graph() -> CompositionV4 {
    CompositionV4 {
        project_id: "project".into(),
        revision: "fixed-r1".into(),
        sequence_id: "scene".into(),
        scope: CompositionScope::Visual,
        time: t(1, 7),
        format: CompiledFormatV4 {
            fps: FrameRate {
                numerator: 30,
                denominator: 1,
            },
            canvas_size: CanvasSizeV4 {
                width: 64,
                height: 48,
            },
            profile: FormatProfileV4::ClassicGlobal,
        },
        tracks: vec![],
        layers: vec![CompiledLayerV4 {
            element_id: "element".into(),
            track_id: "main".into(),
            track_order: 0,
            element_order: 0,
            local_time: t(1, 7),
            content: SampledContentV4::Image {
                asset_id: "asset".into(),
            },
            visual: Some(SampledVisualV4 {
                profile: VisualProfileV4::ClassicCanvasV31,
                transform: TransformV4 {
                    position_x_px: 3.0,
                    position_y_px: -4.0,
                    scale_x: -2.0,
                    scale_y: 0.5,
                    rotation_degrees: 90.0,
                },
                element_opacity: 1.25,
                track_opacity: 1.0,
                effective_opacity: 1.25,
                opacity_policy: LayerOpacityPolicyV4::MultiplyTrack,
                blend_mode: BlendModeV4::Normal,
                effects: vec![],
                masks: vec![],
            }),
        }],
        audio: vec![],
        transitions: vec![],
        fonts: vec![],
        coverage: vec![],
        background: CompiledBackgroundV4::Color {
            color: ColorV4 {
                encoding: ColorEncodingV4::Srgb,
                red: 0.2,
                green: 0.4,
                blue: 0.6,
                alpha: 0.25,
            },
        },
        resources: vec![CasResourceV4 {
            asset_id: "asset".into(),
            version: AssetVersion {
                id: format!("av_{}", "a".repeat(64)),
                content_hash: "a".repeat(64),
                size: 16,
            },
            kind: AssetKind::Image,
            duration: None,
            metadata: MediaMetadataV4 {
                mime_type: None,
                intrinsic_size: Some(CanvasSizeV4 {
                    width: 1,
                    height: 1,
                }),
                frame_rate: None,
                rotation_degrees: None,
                audio: AudioPresenceV4::Absent {},
            },
            license: None,
        }],
    }
}
fn image(id: &str) -> DecodedVisualSourceV4 {
    DecodedVisualSourceV4::ReadyImage {
        element_id: id.into(),
        width: 320,
        height: 90,
    }
}
fn video_graph() -> CompositionV4 {
    let mut g = graph();
    let range = TimeRange {
        start: t(1, 1),
        duration: t(2, 1),
    };
    g.layers[0].content = SampledContentV4::Video {
        asset_id: "asset".into(),
        stream: None,
        source_audio: SourceAudioDecisionV4::ScopeExcluded,
        source: SourceClockV4 {
            timeline_local: t(1, 7),
            source_time: t(5, 4),
            point_profile: SourcePointProfileV4::ClassicVideoRoundedTicks,
            rate: RationalRateV4 {
                numerator: 3,
                denominator: 2,
            },
            mapping: RetimeMappingV4::ClassicRoundedTicks { timebase: 120000 },
            nominal_request: range.clone(),
            requested_read_span: range.clone(),
            declared_resize_extent: Some(t(4, 1)),
            read_window: BoundedReadWindowV4::Known {
                resource_extent: t(4, 1),
                range: Some(range),
            },
            sample: SourceReadStateV4::InBounds,
            eof_policy: EofPolicyV4::RequiresBackendProfile,
        },
    };
    g.resources[0].kind = AssetKind::Video;
    g
}
fn video(timestamp: f64, duration: f64) -> DecodedVisualSourceV4 {
    DecodedVisualSourceV4::ReadyVideo {
        element_id: "element".into(),
        width: 320,
        height: 90,
        requested_time: t(5, 4),
        selected_video_stream: None,
        actual_timestamp_seconds: timestamp,
        frame_duration_seconds: duration,
    }
}
#[test]
fn actual_contain_flip_center_raw_opacity_and_alpha_have_independent_expected_values() {
    let prepared = prepare_visual_frame_v4(&graph()).unwrap();
    let bound = bind_visual_sources_v4(&prepared, &[image("element")]).unwrap();
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
}
#[test]
fn complete_unique_decode_set_preserves_layer_order_and_zero_transform() {
    let mut g = graph();
    let mut second = g.layers[0].clone();
    second.element_id = "second".into();
    second.element_order = 1;
    second.visual.as_mut().unwrap().transform.scale_x = 0.0;
    g.layers.push(second);
    let prepared = prepare_visual_frame_v4(&g).unwrap();
    let bound = bind_visual_sources_v4(&prepared, &[image("second"), image("element")]).unwrap();
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
        assert!(bind_visual_sources_v4(&prepared, &bad).is_err());
    }
}
#[test]
fn video_fixed_point_and_half_open_observation_interval_do_not_hold_gap_frames() {
    let prepared = prepare_visual_frame_v4(&video_graph()).unwrap();
    assert!(bind_visual_sources_v4(&prepared, &[video(1.2, 0.1)]).is_ok());
    for bad in [
        video(1.0, 0.25),
        video(1.26, 0.1),
        video(1.0, f64::NAN),
        video(f64::MAX, f64::MAX),
    ] {
        assert!(bind_visual_sources_v4(&prepared, &[bad]).is_err());
    }
    let mut changed = video(1.2, 0.1);
    if let DecodedVisualSourceV4::ReadyVideo { requested_time, .. } = &mut changed {
        *requested_time = t(3, 2);
    }
    assert!(bind_visual_sources_v4(&prepared, &[changed]).is_err());
    let gap = DecodedVisualSourceV4::NoFrame {
        element_id: "element".into(),
        requested_time: t(5, 4),
        reason: VisualNoFrameReasonV4::TimestampGap,
    };
    assert!(
        bind_visual_sources_v4(&prepared, &[gap]).unwrap().layers[0]
            .quad
            .is_none()
    );
    let outside = DecodedVisualSourceV4::NoFrame {
        element_id: "element".into(),
        requested_time: t(5, 4),
        reason: VisualNoFrameReasonV4::OutsideReadWindow,
    };
    assert!(bind_visual_sources_v4(&prepared, &[outside]).is_err());
}
#[test]
fn unsupported_profiles_stream_selection_and_nonfinite_uniforms_fail_before_decode() {
    let mut g = graph();
    g.format.profile = FormatProfileV4::LegacyPerScene;
    assert!(prepare_visual_frame_v4(&g).is_err());
    let mut g = graph();
    g.layers[0].visual.as_mut().unwrap().blend_mode = BlendModeV4::Multiply;
    assert!(prepare_visual_frame_v4(&g).is_err());
    let mut g = graph();
    g.layers[0].visual.as_mut().unwrap().transform.position_x_px = f64::MAX;
    assert!(prepare_visual_frame_v4(&g).is_err());
    let mut g = video_graph();
    if let SampledContentV4::Video { stream, .. } = &mut g.layers[0].content {
        *stream = Some(0);
    }
    assert!(prepare_visual_frame_v4(&g).is_err());
}
