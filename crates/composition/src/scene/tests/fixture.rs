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
pub(super) fn color() -> Color {
    Color {
        encoding: ColorEncoding::Srgb,
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
pub(super) fn visual() -> VisualStyle {
    VisualStyle {
        profile: VisualProfile::LegacyCompositionV3,
        transform: Transform {
            position_x_px: 0.0,
            position_y_px: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation_degrees: 0.0,
        },
        opacity: 0.8,
        blend_mode: BlendMode::Normal,
        effects: vec![],
        masks: vec![],
    }
}
pub(super) fn track(id: &str, kind: SceneTrackKind, elements: Vec<TimelineElement>) -> SceneTrack {
    SceneTrack {
        id: id.into(),
        name: id.into(),
        kind,
        enabled: true,
        muted: false,
        locked: false,
        opacity: 0.25,
        gain: Gain::Decibels { value: 0.0 },
        elements,
        channels: vec![],
    }
}
pub(super) fn element(id: &str, content: ElementContent) -> TimelineElement {
    TimelineElement {
        id: id.into(),
        name: id.into(),
        timeline_range: range(t(1, 2), t(2, 1)),
        source_span: ElementSourceSpan::LegacySourceRange {
            range: range(t(1, 1), t(2, 1)),
        },
        enabled: true,
        content,
        visual: Some(visual()),
        channels: vec![],
    }
}
pub(super) fn snapshot() -> EditorSnapshot {
    let video = element(
        "video",
        ElementContent::Video {
            asset_id: "asset".into(),
            video_stream: None,
            source_audio_enabled: true,
            audio: AudioStyle {
                stream: None,
                gain: Gain::Decibels { value: 0.0 },
                muted: false,
            },
            retime: Retime {
                rate: RationalRate {
                    numerator: 1,
                    denominator: 1,
                },
                maintain_pitch: true,
                mapping: RetimeMapping::ExactRational,
            },
        },
    );
    let shape = element(
        "shape",
        ElementContent::Graphic {
            graphic: Graphic {
                geometry: GraphicGeometry::LegacyRectangle {
                    width_px: 20.0,
                    height_px: 10.0,
                },
                source_size: CanvasSize {
                    width: 20,
                    height: 10,
                },
                fill: color(),
                stroke: Stroke {
                    color: color(),
                    width_px: 0.0,
                    align: StrokeAlign::Center,
                },
            },
        },
    );
    EditorSnapshot {
        revision: "fixed-r7".into(),
        project: EditorDocument {
            schema_version: 4,
            id: "project".into(),
            name: "project".into(),
            default_sequence_id: "scene".into(),
            settings: SettingsProfile::ClassicGlobal {
                settings: ProjectSettings {
                    fps: FrameRate {
                        numerator: 30000,
                        denominator: 1001,
                    },
                    canvas_size: CanvasSize {
                        width: 160,
                        height: 90,
                    },
                    canvas_size_mode: CanvasSizeMode::Preset,
                    last_custom_canvas_size: None,
                    original_canvas_size: None,
                    background: Background::Color {
                        color: Color {
                            red: 0.0,
                            green: 0.0,
                            blue: 0.0,
                            ..color()
                        },
                    },
                },
            },
            assets: vec![ProjectAsset {
                id: "asset".into(),
                name: "asset".into(),
                kind: AssetKind::Video,
                version: version(),
                duration: Some(t(5, 1)),
                origin: ResourceOrigin::LocalImport {
                    original_name: "fixture.mp4".into(),
                },
                metadata: ProjectMediaMetadata {
                    mime_type: Some("video/mp4".into()),
                    intrinsic_size: Some(CanvasSize {
                        width: 160,
                        height: 90,
                    }),
                    frame_rate: Some(FrameRate {
                        numerator: 30,
                        denominator: 1,
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
            fonts: vec![],
            sequences: vec![Scene {
                id: "scene".into(),
                name: "scene".into(),
                main_track_id: "main".into(),
                tracks: vec![
                    track("main", SceneTrackKind::Video, vec![video]),
                    track("graphics", SceneTrackKind::Graphic, vec![shape]),
                ],
                captions: vec![],
                transitions: vec![],
                bookmarks: vec![],
            }],
        },
    }
}
pub(super) fn key(id: &str, at: Time, value: f64) -> ScalarKey {
    ScalarKey {
        id: id.into(),
        segment_id: None,
        time: at,
        value,
        left_handle: None,
        right_handle: None,
        segment_to_next: ScalarSegment::Bezier,
        tangent_mode: TangentMode::Flat,
    }
}
pub(super) fn channel(
    id: &str,
    target: ChannelTarget,
    p: ScalarParameter,
    keys: Vec<ScalarKey>,
) -> AnimationChannel {
    AnimationChannel::Scalar {
        id: id.into(),
        target,
        binding: ScalarBinding::Property { parameter: p },
        keys,
        extrapolation: Extrapolation {
            before: ExtrapolationMode::Hold,
            after: ExtrapolationMode::Hold,
        },
    }
}
pub(super) fn compile(snapshot: &EditorSnapshot) -> SceneComposition {
    compile_frame_scene(snapshot, "scene", &t(3, 2), CompositionScope::AudioVisual).unwrap()
}
pub(super) fn close(value: f64, want: f64) {
    assert!((value - want).abs() < 1e-10, "expected {want}, got {value}");
}
pub(super) fn excluded(graph: &SceneComposition, id: &str, reason: Exclusion) {
    assert!(
        graph
            .coverage
            .iter()
            .any(|e| e.source_id == id
                && e.disposition == CoverageDisposition::Excluded { reason })
    );
}
