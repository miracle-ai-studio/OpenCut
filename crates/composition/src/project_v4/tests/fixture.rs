use super::super::*;

pub(super) fn t(n: i64, d: u32) -> Time {
    Time {
        ticks: n.to_string(),
        timebase: d,
    }
}
pub(super) fn range(start: Time, duration: Time) -> TimeRange {
    TimeRange { start, duration }
}
pub(super) fn color() -> ColorV4 {
    ColorV4 {
        encoding: ColorEncodingV4::Srgb,
        red: 1.0,
        green: 0.0,
        blue: 0.0,
        alpha: 1.0,
    }
}
pub(super) fn version() -> AssetVersion {
    let hash = "a".repeat(64);
    AssetVersion {
        id: format!("av_{hash}"),
        content_hash: hash,
        size: 1,
    }
}
pub(super) fn visual() -> VisualStyleV4 {
    VisualStyleV4 {
        profile: VisualProfileV4::LegacyCompositionV3,
        transform: TransformV4 {
            position_x_px: 0.0,
            position_y_px: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation_degrees: 0.0,
        },
        opacity: 0.8,
        blend_mode: BlendModeV4::Normal,
        effects: vec![],
        masks: vec![],
    }
}
pub(super) fn track(id: &str, kind: TrackKindV4, elements: Vec<TimelineElementV4>) -> TrackV4 {
    TrackV4 {
        id: id.into(),
        name: id.into(),
        kind,
        enabled: true,
        muted: false,
        locked: false,
        opacity: 0.25,
        gain: GainV4::Decibels { value: 0.0 },
        elements,
        channels: vec![],
    }
}
pub(super) fn element(id: &str, content: ElementContentV4) -> TimelineElementV4 {
    TimelineElementV4 {
        id: id.into(),
        name: id.into(),
        timeline_range: range(t(1, 2), t(2, 1)),
        source_span: ElementSourceSpanV4::LegacySourceRange {
            range: range(t(1, 1), t(2, 1)),
        },
        enabled: true,
        content,
        visual: Some(visual()),
        channels: vec![],
    }
}
pub(super) fn snapshot() -> ProjectSnapshotV4 {
    let video = element(
        "video",
        ElementContentV4::Video {
            asset_id: "asset".into(),
            video_stream: None,
            source_audio_enabled: true,
            audio: AudioStyleV4 {
                stream: None,
                gain: GainV4::Decibels { value: 0.0 },
                muted: false,
            },
            retime: RetimeV4 {
                rate: RationalRateV4 {
                    numerator: 1,
                    denominator: 1,
                },
                maintain_pitch: true,
                mapping: RetimeMappingV4::ExactRational,
            },
        },
    );
    let shape = element(
        "shape",
        ElementContentV4::Graphic {
            graphic: GraphicV4 {
                geometry: GraphicGeometryV4::LegacyRectangle {
                    width_px: 20.0,
                    height_px: 10.0,
                },
                source_size: CanvasSizeV4 {
                    width: 20,
                    height: 10,
                },
                fill: color(),
                stroke: StrokeV4 {
                    color: color(),
                    width_px: 0.0,
                    align: StrokeAlignV4::Center,
                },
            },
        },
    );
    ProjectSnapshotV4 {
        revision: "fixed-r7".into(),
        project: ProjectDocumentV4 {
            schema_version: 4,
            id: "project".into(),
            name: "project".into(),
            default_sequence_id: "scene".into(),
            settings: SettingsProfileV4::ClassicGlobal {
                settings: ProjectSettingsV4 {
                    fps: FrameRate {
                        numerator: 30000,
                        denominator: 1001,
                    },
                    canvas_size: CanvasSizeV4 {
                        width: 160,
                        height: 90,
                    },
                    canvas_size_mode: CanvasSizeModeV4::Preset,
                    last_custom_canvas_size: None,
                    original_canvas_size: None,
                    background: BackgroundV4::Color {
                        color: ColorV4 {
                            red: 0.0,
                            green: 0.0,
                            blue: 0.0,
                            ..color()
                        },
                    },
                },
            },
            assets: vec![AssetV4 {
                id: "asset".into(),
                name: "asset".into(),
                kind: AssetKind::Video,
                version: version(),
                duration: Some(t(5, 1)),
                origin: ResourceOriginV4::LocalImport {
                    original_name: "fixture.mp4".into(),
                },
                metadata: MediaMetadataV4 {
                    mime_type: Some("video/mp4".into()),
                    intrinsic_size: Some(CanvasSizeV4 {
                        width: 160,
                        height: 90,
                    }),
                    frame_rate: Some(FrameRate {
                        numerator: 30,
                        denominator: 1,
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
            fonts: vec![],
            sequences: vec![SequenceV4 {
                id: "scene".into(),
                name: "scene".into(),
                main_track_id: "main".into(),
                tracks: vec![
                    track("main", TrackKindV4::Video, vec![video]),
                    track("graphics", TrackKindV4::Graphic, vec![shape]),
                ],
                captions: vec![],
                transitions: vec![],
                bookmarks: vec![],
            }],
        },
    }
}
pub(super) fn key(id: &str, at: Time, value: f64) -> ScalarKeyV4 {
    ScalarKeyV4 {
        id: id.into(),
        segment_id: None,
        time: at,
        value,
        left_handle: None,
        right_handle: None,
        segment_to_next: ScalarSegmentV4::Bezier,
        tangent_mode: TangentModeV4::Flat,
    }
}
pub(super) fn channel(
    id: &str,
    target: ChannelTargetV4,
    p: ScalarParameterV4,
    keys: Vec<ScalarKeyV4>,
) -> AnimationChannelV4 {
    AnimationChannelV4::Scalar {
        id: id.into(),
        target,
        binding: ScalarBindingV4::Property { parameter: p },
        keys,
        extrapolation: ExtrapolationV4 {
            before: ExtrapolationModeV4::Hold,
            after: ExtrapolationModeV4::Hold,
        },
    }
}
pub(super) fn compile(snapshot: &ProjectSnapshotV4) -> CompositionV4 {
    compile_frame_v4(snapshot, "scene", &t(3, 2), CompositionScope::AudioVisual).unwrap()
}
pub(super) fn close(value: f64, want: f64) {
    assert!((value - want).abs() < 1e-10, "expected {want}, got {value}");
}
pub(super) fn excluded(graph: &CompositionV4, id: &str, reason: ExclusionV4) {
    assert!(
        graph
            .coverage
            .iter()
            .any(|e| e.source_id == id
                && e.disposition == CoverageDispositionV4::Excluded { reason })
    );
}
