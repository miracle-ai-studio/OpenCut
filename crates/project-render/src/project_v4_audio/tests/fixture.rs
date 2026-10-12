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
pub(super) fn limits() -> AudioStageLimitsV4 {
    AudioStageLimitsV4 {
        max_voices: 4,
        max_decoded_bytes: 4 * 1024 * 1024,
        decoder_buffered_blocks: 2,
        decoder_stall_timeout: Duration::from_secs(10),
    }
}
pub(super) fn prepare(s: ProjectSnapshotV4) -> PreparedAudioV4 {
    prepare_audio_v4(
        &s,
        "scene",
        AudioExecutionProfileV4::ClassicV31,
        PcmFormatV4 {
            sample_rate: 48000,
            channels: 2,
        },
    )
    .unwrap()
}
pub(super) fn stage(
    backend: &FfmpegBackend,
    s: ProjectSnapshotV4,
    path: &Path,
    cancel: &CancellationToken,
) -> UnmasteredPcmStageV4 {
    ProjectAudioAdapterV4::prepare_unmastered(backend, prepare(s), limits(), cancel, |r| {
        assert_eq!(r.asset_id, "asset");
        Ok(path.to_owned())
    })
    .unwrap()
}
pub(super) fn audio_mut(s: &mut ProjectSnapshotV4) -> &mut AudioStyleV4 {
    let ElementContentV4::Audio { audio, .. } =
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
) -> ProjectSnapshotV4 {
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
        revision: "fixed-native-audio".into(),
        project: ProjectDocumentV4 {
            schema_version: 4,
            id: "project".into(),
            name: "audio fixture".into(),
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
                    background: BackgroundV4::Color {
                        color: ColorV4 {
                            encoding: ColorEncodingV4::Srgb,
                            red: 0.0,
                            green: 0.0,
                            blue: 0.0,
                            alpha: 1.0,
                        },
                    },
                },
            },
            assets: vec![AssetV4 {
                id: "asset".into(),
                name: "synthetic".into(),
                kind: AssetKind::Audio,
                version: AssetVersion {
                    id: format!("av_{hash}"),
                    content_hash: hash,
                    size: 16,
                },
                duration: None,
                origin: ResourceOriginV4::LocalImport {
                    original_name: "fixture.wav".into(),
                },
                metadata: MediaMetadataV4 {
                    mime_type: Some("audio/wav".into()),
                    intrinsic_size: None,
                    frame_rate: None,
                    rotation_degrees: None,
                    audio: AudioPresenceV4::Present {
                        metadata: Some(AudioMetadataV4 {
                            sample_rate: 48000,
                            channels,
                            channel_layout: None,
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
                    track("main", TrackKindV4::Video, vec![]),
                    track(
                        "sound",
                        TrackKindV4::Audio,
                        vec![TimelineElementV4 {
                            id: "voice".into(),
                            name: "voice".into(),
                            timeline_range: TimeRange {
                                start: t(0, 1),
                                duration: t(frames, 48000),
                            },
                            source_span: ElementSourceSpanV4::ClassicTrimmed {
                                trim_start: t(0, 1),
                                trim_end: t(0, 1),
                                source_duration: None,
                            },
                            enabled: true,
                            content: ElementContentV4::Audio {
                                asset_id: "asset".into(),
                                audio: AudioStyleV4 {
                                    stream: None,
                                    gain: GainV4::Decibels { value: 0.0 },
                                    muted: false,
                                },
                                retime: RetimeV4 {
                                    rate: RationalRateV4 {
                                        numerator,
                                        denominator,
                                    },
                                    maintain_pitch: false,
                                    mapping: RetimeMappingV4::ClassicRoundedTicks {
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
