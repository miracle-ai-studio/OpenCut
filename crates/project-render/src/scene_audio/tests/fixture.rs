use super::*;
pub(super) fn t(n: i64, d: u32) -> Time {
    Time {
        ticks: n.to_string(),
        timebase: d,
    }
}
pub(super) fn close(actual: f32, expected: f64) {
    assert!(
        (f64::from(actual) - expected).abs() < 1e-7,
        "{actual} != {expected}"
    );
}
pub(super) fn limits() -> AudioStageLimits {
    AudioStageLimits {
        max_voices: 4,
        max_decoded_bytes: 4 * 1024 * 1024,
        decoder_buffered_blocks: 2,
        decoder_stall_timeout: Duration::from_secs(10),
    }
}
pub(super) fn prepare(s: EditorSnapshot) -> PreparedAudio {
    prepare_audio_scene(
        &s,
        "scene",
        AudioExecutionProfile::ClassicV31,
        PcmFormat {
            sample_rate: 48000,
            channels: 2,
        },
    )
    .unwrap()
}
pub(super) fn stage(
    backend: &FfmpegBackend,
    s: EditorSnapshot,
    path: &Path,
    cancel: &CancellationToken,
) -> UnmasteredPcmStage {
    ProjectAudioAdapter::prepare_unmastered(backend, prepare(s), limits(), cancel, |r| {
        assert_eq!(r.asset_id, "asset");
        Ok(path.to_owned())
    })
    .unwrap()
}
pub(super) fn audio_mut(s: &mut EditorSnapshot) -> &mut AudioStyle {
    let ElementContent::Audio { audio, .. } =
        &mut s.project.sequences[0].tracks[1].elements[0].content
    else {
        panic!()
    };
    audio
}
pub(super) fn wav(path: &Path, channels: u16, samples: &[i16]) {
    assert_eq!(samples.len() % channels as usize, 0);
    let length = (samples.len() * 2) as u32;
    let mut bytes = vec![];
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + length).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&48000u32.to_le_bytes());
    bytes.extend_from_slice(&(48000u32 * u32::from(channels) * 2).to_le_bytes());
    bytes.extend_from_slice(&(channels * 2).to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&length.to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    fs::write(path, bytes).unwrap();
}
pub(super) fn snapshot(
    channels: u32,
    frames: i64,
    numerator: u32,
    denominator: u32,
) -> EditorSnapshot {
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
        revision: "fixed-native-audio".into(),
        project: EditorDocument {
            cover: None,
            schema_version: 4,
            id: "project".into(),
            name: "audio fixture".into(),
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
                    background: Background::Color {
                        color: Color {
                            encoding: ColorEncoding::Srgb,
                            red: 0.0,
                            green: 0.0,
                            blue: 0.0,
                            alpha: 1.0,
                        },
                    },
                },
            },
            assets: vec![ProjectAsset {
                id: "asset".into(),
                name: "synthetic".into(),
                kind: AssetKind::Audio,
                version: AssetVersion {
                    id: format!("av_{hash}"),
                    content_hash: hash,
                    size: 16,
                },
                duration: None,
                origin: ResourceOrigin::LocalImport {
                    original_name: "fixture.wav".into(),
                },
                metadata: ProjectMediaMetadata {
                    mime_type: Some("audio/wav".into()),
                    intrinsic_size: None,
                    frame_rate: None,
                    rotation_degrees: None,
                    audio: AudioPresence::Present {
                        metadata: Some(AudioMetadata {
                            sample_rate: 48000,
                            channels,
                            channel_layout: None,
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
                    track("main", SceneTrackKind::Video, vec![]),
                    track(
                        "sound",
                        SceneTrackKind::Audio,
                        vec![TimelineElement {
                            id: "voice".into(),
                            name: "voice".into(),
                            timeline_range: TimeRange {
                                start: t(0, 1),
                                duration: t(frames, 48000),
                            },
                            source_span: ElementSourceSpan::ClassicTrimmed {
                                trim_start: t(0, 1),
                                trim_end: t(0, 1),
                                source_duration: None,
                            },
                            enabled: true,
                            content: ElementContent::Audio {
                                asset_id: "asset".into(),
                                audio: AudioStyle {
                                    stream: None,
                                    gain: Gain::Decibels { value: 0.0 },
                                    muted: false,
                                },
                                retime: Retime {
                                    rate: RationalRate {
                                        numerator,
                                        denominator,
                                    },
                                    maintain_pitch: false,
                                    mapping: RetimeMapping::ClassicRoundedTicks {
                                        timebase: 120000,
                                    },
                                },
                            },
                            visual: None,
                            channels: vec![],
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
