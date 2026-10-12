use super::*;
pub(super) fn t(n: i64, d: u32) -> Time {
    Time {
        ticks: n.to_string(),
        timebase: d,
    }
}
pub(super) fn key(id: &str, time: Time, value: f64) -> ScalarKeyV4 {
    ScalarKeyV4 {
        id: id.into(),
        segment_id: None,
        time,
        value,
        left_handle: None,
        right_handle: None,
        segment_to_next: ScalarSegmentV4::Linear,
        tangent_mode: TangentModeV4::Auto,
    }
}
pub(super) fn channel(track: bool, keys: Vec<ScalarKeyV4>) -> AnimationChannelV4 {
    AnimationChannelV4::Scalar {
        id: if track { "track-gain" } else { "element-gain" }.into(),
        target: if track {
            ChannelTargetV4::Track {
                track_id: "sound".into(),
            }
        } else {
            ChannelTargetV4::Element {
                element_id: "voice".into(),
            }
        },
        binding: ScalarBindingV4::Property {
            parameter: ScalarParameterV4::GainDecibels,
        },
        keys,
        extrapolation: ExtrapolationV4 {
            before: ExtrapolationModeV4::Hold,
            after: ExtrapolationModeV4::Hold,
        },
    }
}
pub(super) fn audio_mut(s: &mut ProjectSnapshotV4) -> &mut AudioStyleV4 {
    let ElementContentV4::Audio { audio, .. } =
        &mut s.project.sequences[0].tracks[1].elements[0].content
    else {
        panic!()
    };
    audio
}
pub(super) fn as_video(s: &mut ProjectSnapshotV4) {
    s.project.assets[0].kind = AssetKind::Video;
    let track = &mut s.project.sequences[0].tracks[1];
    track.kind = TrackKindV4::Video;
    let e = &mut track.elements[0];
    let ElementContentV4::Audio {
        asset_id,
        audio,
        retime,
    } = &e.content
    else {
        panic!()
    };
    e.content = ElementContentV4::Video {
        asset_id: asset_id.clone(),
        audio: audio.clone(),
        retime: retime.clone(),
        video_stream: None,
        source_audio_enabled: true,
    };
    e.visual = Some(VisualStyleV4 {
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
    });
}
pub(super) fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-10, "{a} != {b}");
}
pub(super) fn prepare(s: &ProjectSnapshotV4, rate: u32) -> PreparedAudioV4 {
    prepare_audio_v4(
        s,
        "scene",
        AudioExecutionProfileV4::ClassicV31,
        PcmFormatV4 {
            sample_rate: rate,
            channels: 2,
        },
    )
    .unwrap()
}
pub(super) fn snapshot() -> ProjectSnapshotV4 {
    let black = ColorV4 {
        encoding: ColorEncodingV4::Srgb,
        red: 0.0,
        green: 0.0,
        blue: 0.0,
        alpha: 1.0,
    };
    let track = |id: &str, kind, elements| TrackV4 {
        id: id.into(),
        name: id.into(),
        kind,
        enabled: true,
        muted: false,
        locked: false,
        opacity: 1.0,
        gain: GainV4::Decibels { value: 0.0 },
        elements,
        channels: vec![],
    };
    let hash = "a".repeat(64);
    ProjectSnapshotV4 {
        revision: "fixed-audio-r1".into(),
        project: ProjectDocumentV4 {
            schema_version: 4,
            id: "project".into(),
            name: "audio".into(),
            default_sequence_id: "scene".into(),
            settings: SettingsProfileV4::ClassicGlobal {
                settings: ProjectSettingsV4 {
                    fps: FrameRate {
                        numerator: 30,
                        denominator: 1,
                    },
                    canvas_size: CanvasSizeV4 {
                        width: 160,
                        height: 90,
                    },
                    canvas_size_mode: CanvasSizeModeV4::Preset,
                    last_custom_canvas_size: None,
                    original_canvas_size: None,
                    background: BackgroundV4::Color { color: black },
                },
            },
            assets: vec![AssetV4 {
                id: "asset".into(),
                name: "sound".into(),
                kind: AssetKind::Audio,
                version: AssetVersion {
                    id: format!("av_{hash}"),
                    content_hash: hash,
                    size: 16,
                },
                duration: Some(t(2, 1)),
                origin: ResourceOriginV4::LegacyLocator {
                    uri: "/private/audio-fixture.wav".into(),
                },
                license: None,
                metadata: MediaMetadataV4 {
                    mime_type: Some("audio/wav".into()),
                    intrinsic_size: None,
                    frame_rate: None,
                    rotation_degrees: None,
                    audio: AudioPresenceV4::Present {
                        metadata: Some(AudioMetadataV4 {
                            sample_rate: 48000,
                            channels: 1,
                            channel_layout: Some("mono".into()),
                        }),
                    },
                },
            }],
            fonts: vec![],
            sequences: vec![SequenceV4 {
                id: "scene".into(),
                name: "scene".into(),
                main_track_id: "main".into(),
                tracks: vec![
                    track("main", TrackKindV4::Video, vec![]),
                    track(
                        "sound",
                        TrackKindV4::Audio,
                        vec![TimelineElementV4 {
                            id: "voice".into(),
                            name: "voice".into(),
                            enabled: true,
                            visual: None,
                            channels: vec![],
                            timeline_range: TimeRange {
                                start: t(3, 8),
                                duration: t(1, 2),
                            },
                            source_span: ElementSourceSpanV4::ClassicTrimmed {
                                trim_start: t(1, 4),
                                trim_end: t(0, 1),
                                source_duration: None,
                            },
                            content: ElementContentV4::Audio {
                                asset_id: "asset".into(),
                                audio: AudioStyleV4 {
                                    stream: None,
                                    gain: GainV4::Decibels { value: 0.0 },
                                    muted: false,
                                },
                                retime: RetimeV4 {
                                    rate: RationalRateV4 {
                                        numerator: 3,
                                        denominator: 2,
                                    },
                                    maintain_pitch: false,
                                    mapping: RetimeMappingV4::ClassicRoundedTicks {
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
