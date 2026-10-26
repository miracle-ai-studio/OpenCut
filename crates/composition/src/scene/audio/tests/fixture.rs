use super::*;
pub(super) fn t(n: i64, d: u32) -> Time {
    Time {
        ticks: n.to_string(),
        timebase: d,
    }
}
pub(super) fn key(id: &str, time: Time, value: f64) -> ScalarKey {
    ScalarKey {
        id: id.into(),
        segment_id: None,
        time,
        value,
        left_handle: None,
        right_handle: None,
        segment_to_next: ScalarSegment::Linear,
        tangent_mode: TangentMode::Auto,
    }
}
pub(super) fn channel(track: bool, keys: Vec<ScalarKey>) -> AnimationChannel {
    AnimationChannel::Scalar {
        id: if track { "track-gain" } else { "element-gain" }.into(),
        target: if track {
            ChannelTarget::Track {
                track_id: "sound".into(),
            }
        } else {
            ChannelTarget::Element {
                element_id: "voice".into(),
            }
        },
        binding: ScalarBinding::Property {
            parameter: ScalarParameter::GainDecibels,
        },
        keys,
        extrapolation: Extrapolation {
            before: ExtrapolationMode::Hold,
            after: ExtrapolationMode::Hold,
        },
    }
}
pub(super) fn audio_mut(s: &mut EditorSnapshot) -> &mut AudioStyle {
    let ElementContent::Audio { audio, .. } =
        &mut s.project.sequences[0].tracks[1].elements[0].content
    else {
        panic!()
    };
    audio
}
pub(super) fn as_video(s: &mut EditorSnapshot) {
    s.project.assets[0].kind = AssetKind::Video;
    let track = &mut s.project.sequences[0].tracks[1];
    track.kind = SceneTrackKind::Video;
    let e = &mut track.elements[0];
    let ElementContent::Audio {
        asset_id,
        audio,
        retime,
    } = &e.content
    else {
        panic!()
    };
    e.content = ElementContent::Video {
        asset_id: asset_id.clone(),
        audio: audio.clone(),
        retime: retime.clone(),
        video_stream: None,
        source_audio_enabled: true,
    };
    e.visual = Some(VisualStyle {
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
    });
}
pub(super) fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-10, "{a} != {b}");
}
pub(super) fn prepare(s: &EditorSnapshot, rate: u32) -> PreparedAudio {
    prepare_audio_scene(
        s,
        "scene",
        AudioExecutionProfile::ClassicV31,
        PcmFormat {
            sample_rate: rate,
            channels: 2,
        },
    )
    .unwrap()
}
pub(super) fn snapshot() -> EditorSnapshot {
    let black = Color {
        encoding: ColorEncoding::Srgb,
        red: 0.0,
        green: 0.0,
        blue: 0.0,
        alpha: 1.0,
    };
    let track = |id: &str, kind, elements| SceneTrack {
        id: id.into(),
        name: id.into(),
        kind,
        enabled: true,
        muted: false,
        locked: false,
        opacity: 1.0,
        gain: Gain::Decibels { value: 0.0 },
        elements,
        channels: vec![],
    };
    let hash = "a".repeat(64);
    EditorSnapshot {
        revision: "fixed-audio-r1".into(),
        project: EditorDocument {
            cover: None,
            schema_version: 4,
            id: "project".into(),
            name: "audio".into(),
            default_sequence_id: "scene".into(),
            settings: SettingsProfile::ClassicGlobal {
                settings: ProjectSettings {
                    fps: FrameRate {
                        numerator: 30,
                        denominator: 1,
                    },
                    canvas_size: CanvasSize {
                        width: 160,
                        height: 90,
                    },
                    canvas_size_mode: CanvasSizeMode::Preset,
                    last_custom_canvas_size: None,
                    original_canvas_size: None,
                    background: Background::Color { color: black },
                },
            },
            assets: vec![ProjectAsset {
                id: "asset".into(),
                name: "sound".into(),
                kind: AssetKind::Audio,
                version: AssetVersion {
                    id: format!("av_{hash}"),
                    content_hash: hash,
                    size: 16,
                },
                duration: Some(t(2, 1)),
                origin: ResourceOrigin::LegacyLocator {
                    uri: "/private/audio-fixture.wav".into(),
                },
                license: None,
                metadata: ProjectMediaMetadata {
                    mime_type: Some("audio/wav".into()),
                    intrinsic_size: None,
                    frame_rate: None,
                    rotation_degrees: None,
                    audio: AudioPresence::Present {
                        metadata: Some(AudioMetadata {
                            sample_rate: 48000,
                            channels: 1,
                            channel_layout: Some("mono".into()),
                        }),
                    },
                },
            }],
            fonts: vec![],
            sequences: vec![Scene {
                id: "scene".into(),
                name: "scene".into(),
                main_track_id: "main".into(),
                tracks: vec![
                    track("main", SceneTrackKind::Video, vec![]),
                    track(
                        "sound",
                        SceneTrackKind::Audio,
                        vec![TimelineElement {
                            id: "voice".into(),
                            name: "voice".into(),
                            enabled: true,
                            visual: None,
                            channels: vec![],
                            timeline_range: TimeRange {
                                start: t(3, 8),
                                duration: t(1, 2),
                            },
                            source_span: ElementSourceSpan::ClassicTrimmed {
                                trim_start: t(1, 4),
                                trim_end: t(0, 1),
                                source_duration: None,
                            },
                            content: ElementContent::Audio {
                                asset_id: "asset".into(),
                                audio: AudioStyle {
                                    stream: None,
                                    gain: Gain::Decibels { value: 0.0 },
                                    muted: false,
                                },
                                retime: Retime {
                                    rate: RationalRate {
                                        numerator: 3,
                                        denominator: 2,
                                    },
                                    maintain_pitch: false,
                                    mapping: RetimeMapping::ClassicRoundedTicks {
                                        timebase: 120000,
                                    },
                                },
                            },
                        }],
                    ),
                ],
                captions: vec![],
                transitions: vec![],
                bookmarks: vec![],
            }],
        },
    }
}
