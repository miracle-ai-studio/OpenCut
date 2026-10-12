use super::*;
use opencut_composition::project_v4::PreviewGainOverrideV4;
use opencut_contracts::Time;

fn time(n: i64, d: u32) -> Time {
    Time {
        ticks: n.to_string(),
        timebase: d,
    }
}
fn audio_snapshot() -> opencut_contracts::project_v4::ProjectSnapshotV4 {
    use opencut_contracts::{
        AssetKind, AssetVersion, CreateProject, FrameRate, TimeRange, project_v4::*,
    };
    let mut project = crate::project_v4::new_document(&CreateProject {
        id: "p".into(),
        name: "Sound".into(),
        sequence_id: "s".into(),
        width: 16,
        height: 16,
        fps: FrameRate {
            numerator: 30,
            denominator: 1,
        },
    })
    .unwrap();
    project.assets.push(AssetV4 {
        id: "asset".into(),
        name: "Audio".into(),
        kind: AssetKind::Audio,
        version: AssetVersion {
            id: format!("av_{}", "a".repeat(64)),
            content_hash: "a".repeat(64),
            size: 16,
        },
        duration: Some(time(1001, 1)),
        origin: ResourceOriginV4::LegacyLocator {
            uri: "/private/not-public.wav".into(),
        },
        license: None,
        metadata: MediaMetadataV4 {
            mime_type: Some("audio/wav".into()),
            intrinsic_size: None,
            frame_rate: None,
            rotation_degrees: None,
            audio: AudioPresenceV4::Present { metadata: None },
        },
    });
    project.sequences[0].tracks.push(TrackV4 {
        id: "sound".into(),
        name: "Sound".into(),
        kind: TrackKindV4::Audio,
        enabled: true,
        muted: false,
        locked: false,
        opacity: 1.0,
        gain: GainV4::Decibels { value: 0.0 },
        channels: vec![],
        elements: vec![TimelineElementV4 {
            id: "voice".into(),
            name: "Voice".into(),
            timeline_range: TimeRange {
                start: time(0, 1),
                duration: time(1000, 1),
            },
            source_span: ElementSourceSpanV4::ClassicTrimmed {
                trim_start: time(0, 1),
                trim_end: time(0, 1),
                source_duration: None,
            },
            enabled: true,
            visual: None,
            channels: vec![],
            content: ElementContentV4::Audio {
                asset_id: "asset".into(),
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
                    maintain_pitch: false,
                    mapping: RetimeMappingV4::ClassicRoundedTicks { timebase: 120000 },
                },
            },
        }],
    });
    ProjectSnapshotV4 {
        project,
        revision: "fixed".into(),
    }
}
fn machine() -> AudioPreviewMachineV4 {
    AudioPreviewMachineV4::new(
        &AudioPreviewCreateV4 {
            project_id: "p".into(),
            revision: "fixed".into(),
            sequence_id: "s".into(),
            sample_rate: 48000,
            start_time: time(1, 3),
            idempotency_key: "create".into(),
        },
        "session".into(),
        "owner".into(),
        "lease".into(),
        96000,
        0,
    )
    .unwrap()
}
fn control(
    m: &AudioPreviewMachineV4,
    key: &str,
    command: AudioPreviewCommandV4,
) -> AudioPreviewControlV4 {
    let v = m.snapshot(0);
    AudioPreviewControlV4 {
        session_id: v.session_id,
        generation: v.generation,
        owner_id: v.owner_id,
        lease_id: v.lease_id,
        control_id: key.into(),
        command,
    }
}
fn report(m: &mut AudioPreviewMachineV4, key: &str, count: u64, sequence: u64) {
    let input = control(
        m,
        key,
        AudioPreviewCommandV4::ReportConsumed {
            sequence: sequence.to_string(),
            consumed_frames: count.to_string(),
            playing: false,
            stalled: false,
        },
    );
    m.control(&input, 10).unwrap();
}
#[test]
fn credit_and_final_consumption_are_distinct_exact_clocks() {
    let mut m = machine();
    assert_eq!(m.actual_start_sample(), 16000);
    let play = control(&m, "play", AudioPreviewCommandV4::Play);
    m.control(&play, 1).unwrap();
    for start in [16000, 20096] {
        m.produced(m.generation(), m.overlay_epoch(), start, 4096, 2)
            .unwrap();
    }
    assert_eq!(m.next_block(3).unwrap(), None);
    assert_eq!(m.snapshot(3).consumed_frames, "0");
    report(&mut m, "consume", 1024, 0);
    assert_eq!(m.next_block(10).unwrap(), None);
    assert_eq!(m.snapshot(10).authoritative_time, time(17024, 48000));
    let too_far = control(
        &m,
        "false-produced",
        AudioPreviewCommandV4::ReportConsumed {
            sequence: "1".into(),
            consumed_frames: "8193".into(),
            playing: true,
            stalled: false,
        },
    );
    let unchanged = m.snapshot(10);
    assert!(m.control(&too_far, 10).is_err());
    assert_eq!(m.snapshot(10), unchanged);
    report(&mut m, "consume-first-block", 4096, 1);
    assert_eq!(m.next_block(10).unwrap(), Some((24192, 4096)));
    assert!(m.disconnect(m.generation(), 11).unwrap());
    assert_eq!(m.snapshot(11).state, AudioPreviewStateV4::Paused);
    report(&mut m, "final", 5120, 2);
    let resume = control(&m, "resume", AudioPreviewCommandV4::Play);
    m.control(&resume, 12).unwrap();
    assert_eq!(m.actual_start_sample(), 21120);
    assert_eq!(m.snapshot(12).consumed_frames, "0");
}
#[test]
fn validated_overlay_reanchors_after_report_and_replay_never_restarts() {
    let mut m = machine();
    let play = control(&m, "play", AudioPreviewCommandV4::Play);
    m.control(&play, 1).unwrap();
    m.produced(m.generation(), 0, 16000, 1000, 2).unwrap();
    report(&mut m, "consume", 731, 0);
    let old = m.generation();
    let gain = TemporaryAudioGainV4 {
        track_id: "t".into(),
        element_id: "e".into(),
        decibels_tenths: -60,
        gesture_id: "gesture".into(),
        update_sequence: "1".into(),
    };
    let input = control(
        &m,
        "gain",
        AudioPreviewCommandV4::SetTemporaryGain { gain: gain.clone() },
    );
    assert!(m.control(&input, 11).is_err());
    assert_eq!(m.generation(), old);
    let valid = PreviewGainOverrideV4 {
        track_id: gain.track_id.clone(),
        element_id: gain.element_id.clone(),
        decibels_tenths: -60,
    };
    let response = m.control_validated(&input, Some(&valid), 11).unwrap();
    assert_eq!(response.effect, AudioPreviewEffectV4::Replace);
    assert_eq!(response.session.state, AudioPreviewStateV4::Playing);
    assert_eq!(m.actual_start_sample(), 16731);
    assert_eq!(m.overlay_epoch(), 1);
    let replay = m.control(&input, 12).unwrap();
    assert_eq!(replay.effect, AudioPreviewEffectV4::None);
    assert_eq!(replay.session, response.session);
    assert_eq!(
        m.produced(old, 0, 17000, 2, 12).unwrap_err().code,
        "PREVIEW_GENERATION_CONFLICT"
    );
    let mut changed = input.clone();
    changed.command = AudioPreviewCommandV4::Stop;
    assert_eq!(
        m.control(&changed, 12).unwrap_err().code,
        "IDEMPOTENCY_KEY_REUSED"
    );
    let clear = control(
        &m,
        "clear",
        AudioPreviewCommandV4::ClearTemporaryGain {
            gesture_id: "gesture".into(),
        },
    );
    m.control(&clear, 13).unwrap();
    assert!(m.temporary_gain().is_none());
    assert_eq!(m.overlay_epoch(), 2);
}
#[test]
fn invalid_seek_owner_and_expiry_preserve_or_fence_the_right_generation() {
    let mut m = machine();
    let play = control(&m, "play", AudioPreviewCommandV4::Play);
    m.control(&play, 1).unwrap();
    let old = m.snapshot(2);
    let seek = control(
        &m,
        "bad",
        AudioPreviewCommandV4::Seek {
            sequence_time: time(2, 1),
        },
    );
    assert!(m.control(&seek, 2).is_err());
    assert_eq!(m.snapshot(2), old);
    let mut foreign = control(&m, "foreign", AudioPreviewCommandV4::Stop);
    foreign.owner_id = "other".into();
    assert_eq!(m.control(&foreign, 2).unwrap_err().code, "NOT_AUTHORIZED");
    assert!(m.expire(30000).unwrap());
    assert_eq!(m.snapshot(30000).state, AudioPreviewStateV4::Stopped);
    let acquire = AudioPreviewAcquireV4 {
        session_id: "session".into(),
        expected_generation: m.generation().to_string(),
        control_id: "takeover".into(),
    };
    let next = m
        .acquire(&acquire, "new-owner".into(), "new-lease".into(), 30001)
        .unwrap();
    assert_eq!(next.session.state, AudioPreviewStateV4::Paused);
    assert_eq!(next.session.identity.revision, "fixed");
    let stop = control(&m, "stop", AudioPreviewCommandV4::Stop);
    m.control(&stop, 30002).unwrap();
    let take = AudioPreviewAcquireV4 {
        expected_generation: m.generation().to_string(),
        control_id: "after-stop".into(),
        ..acquire
    };
    assert_eq!(
        m.acquire(&take, "third".into(), "third-lease".into(), 60002)
            .unwrap_err()
            .code,
        "PREVIEW_STOPPED"
    );
}
#[test]
fn floor_once_returns_sample_time_and_no_fifo_credit_is_granted_while_paused() {
    let mut m = machine();
    let input = control(
        &m,
        "seek",
        AudioPreviewCommandV4::Seek {
            sequence_time: time(1, 7),
        },
    );
    let view = m.control(&input, 1).unwrap().session;
    assert_eq!(view.actual_start_sample, "6857");
    assert_eq!(view.authoritative_time, time(6857, 48000));
    assert_eq!(m.next_block(2).unwrap(), None);
    let json = serde_json::to_value(view).unwrap();
    assert!(json.get("paths").is_none());
    assert_eq!(json["identity"]["profile"], "classic-realtime-v31");
}
#[test]
fn preview_admits_actual_native_pcm_without_whole_scene_output_and_refuses_unverified_downmix() {
    use crate::classic_audio::*;
    use opencut_composition::project_v4::*;
    struct Resources {
        checkpoints: usize,
        revoked: bool,
    }
    impl ClassicAudioResources for Resources {
        fn resolve(
            &mut self,
            _: &AudioResource,
            _: &str,
        ) -> Result<(String, u64), opencut_contracts::EditorError> {
            unreachable!()
        }
        fn read(
            &mut self,
            _: &str,
            _: u64,
            _: u32,
        ) -> Result<(Vec<u8>, bool), opencut_contracts::EditorError> {
            unreachable!()
        }
        fn release(&mut self, _: &str) -> Result<(), opencut_contracts::EditorError> {
            unreachable!()
        }
        fn checkpoint(&mut self) -> Result<(), opencut_contracts::EditorError> {
            self.checkpoints += 1;
            if self.revoked {
                return Err(opencut_contracts::EditorError::new(
                    "NOT_AUTHORIZED",
                    "Preview read authority revoked",
                ));
            }
            Ok(())
        }
    }
    let p = PreparedPreviewAudioV4::new(
        &audio_snapshot(),
        "s",
        PcmFormatV4 {
            sample_rate: 48000,
            channels: 2,
        },
    )
    .unwrap();
    assert!(ClassicAudioSession::new(p.audio(), 1024 * 1024, 1024).is_err());
    let mut session = ClassicAudioSession::new_preview(&p, 1024 * 1024, 1024).unwrap();
    let resource = session.resource("asset").unwrap();
    let mut resources = Resources {
        checkpoints: 0,
        revoked: false,
    };
    let primary = ClassicAudioPrimarySelection {
        track_id: 2,
        codec: "pcm-s16".into(),
        sample_rate: 44100,
        channels: 6,
    };
    let input = AudioCallback::Admit(DecodeAdmission {
        resource: resource.clone(),
        primary: Some(primary.clone()),
    });
    assert_eq!(
        session.callback(input, &mut resources).unwrap_err().code,
        "UNSUPPORTED_AUDIO_PREVIEW_CHANNEL_LAYOUT"
    );
    let primary = ClassicAudioPrimarySelection {
        channels: 1,
        ..primary
    };
    session
        .callback(
            AudioCallback::Admit(DecodeAdmission {
                resource: resource.clone(),
                primary: Some(primary.clone()),
            }),
            &mut resources,
        )
        .unwrap();
    let observation = PreviewDecodedObservation {
        resource,
        sample_rate: 44100,
        frame_count: "1024".into(),
        channels: 1,
    };
    session
        .callback(
            AudioCallback::ObservePreviewDecoded(observation.clone()),
            &mut resources,
        )
        .unwrap();
    assert!(
        session
            .callback(
                AudioCallback::ObservePreviewDecoded(PreviewDecodedObservation {
                    frame_count: "1025".into(),
                    ..observation
                }),
                &mut resources
            )
            .is_err()
    );
    let format = PreviewSourceFormatV4 {
        element_id: "voice".into(),
        sample_rate: 44100,
        frame_count: 1024,
        channels: 1,
        prepared: false,
        segments: vec![PreviewAudioSegmentV4 {
            segment_id: 3,
            timestamp_seconds: 0.0,
            frame_count: 1024,
        }],
    };
    session
        .callback(
            AudioCallback::BindPreview(BindPreviewSources {
                source_formats: vec![format.clone()],
            }),
            &mut resources,
        )
        .unwrap();
    let block = session
        .callback(
            AudioCallback::PreviewBlock(PreviewBlockRequest {
                start_sample: "0".into(),
                frame_count: 1,
            }),
            &mut resources,
        )
        .unwrap();
    assert_eq!(block["voices"][0]["positions"][0]["segmentId"], 3);
    session
        .set_preview_state(
            0,
            Some(p.validate_temporary_gain("sound", "voice", -60).unwrap()),
        )
        .unwrap();
    let before = resources.checkpoints;
    let typed = session
        .compile_preview_block_typed(
            PreviewBlockRequest {
                start_sample: "1".into(),
                frame_count: 7,
            },
            &mut resources,
        )
        .unwrap();
    assert_eq!(resources.checkpoints, before + 1);
    let legacy_callback = session
        .callback(
            AudioCallback::PreviewBlock(PreviewBlockRequest {
                start_sample: "1".into(),
                frame_count: 7,
            }),
            &mut resources,
        )
        .unwrap();
    assert_eq!(resources.checkpoints, before + 2);
    assert_eq!(serde_json::to_value(&typed).unwrap(), legacy_callback);
    assert_eq!(typed.voices[0].positions[0].as_ref().unwrap().segment_id, 3);
    assert_eq!(
        typed.voices[0].positions[0].as_ref().unwrap().fraction,
        147.0 / 160.0
    );
    assert!(
        typed.voices[0]
            .amplitudes
            .iter()
            .all(|gain| *gain == 10f64.powf(-6.0 / 20.0))
    );
    resources.revoked = true;
    assert_eq!(
        session
            .compile_preview_block_typed(
                PreviewBlockRequest {
                    start_sample: "1".into(),
                    frame_count: 7,
                },
                &mut resources
            )
            .unwrap_err()
            .code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(resources.checkpoints, before + 3);
    assert_eq!(
        session
            .callback(
                AudioCallback::PreviewBlock(PreviewBlockRequest {
                    start_sample: "1".into(),
                    frame_count: 7,
                }),
                &mut resources
            )
            .unwrap_err()
            .code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(resources.checkpoints, before + 4);
    let mut receipt = ClassicAudioPreviewReceipt {
        identity: ClassicAudioIdentity {
            project_id: "p".into(),
            sequence_id: "s".into(),
            revision: "fixed".into(),
            profile: CLASSIC_REALTIME_AUDIO_PROFILE_V4.into(),
            sample_rate: 48000,
            channels: 2,
        },
        engine_identity: "native-verified-runtime".into(),
        decoders: vec![ClassicAudioPreviewDecoderReceipt {
            asset_id: "asset".into(),
            primary,
            sample_rate: 44100,
            frame_count: "1024".into(),
            channels: 1,
        }],
        source_formats: vec![format],
        preparation: vec![],
        peak_accounted_bytes: 8192 * 8 + 1024 * 4,
    };
    session.validate_preview_receipt(&receipt).unwrap();
    receipt.decoders[0].primary.track_id = 1;
    assert!(session.validate_preview_receipt(&receipt).is_err());
}

#[test]
fn picture_token_preserves_owner_expiry_and_generation_without_opening_ready_pcm() {
    let mut m = machine();
    let v = m.snapshot(0);
    let input = AudioPreviewStreamV4 {
        session_id: v.session_id,
        generation: v.generation,
        overlay_epoch: v.overlay_epoch,
        owner_id: v.owner_id,
        lease_id: v.lease_id,
    };
    m.verify_visual_frame(&input, "owner", 0).unwrap();
    assert_eq!(
        m.verify_stream(&input, "owner", 0).unwrap_err().code,
        "PREVIEW_NOT_PLAYING"
    );
    assert_eq!(
        m.verify_visual_frame(&input, "other", 0).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(
        m.verify_visual_frame(&input, "owner", AUDIO_PREVIEW_OWNER_LEASE_MS)
            .unwrap_err()
            .code,
        "PREVIEW_EXPIRED"
    );
    m.control(
        &control(
            &m,
            "seek-picture",
            AudioPreviewCommandV4::Seek {
                sequence_time: time(1, 1),
            },
        ),
        1,
    )
    .unwrap();
    assert_eq!(
        m.verify_visual_frame(&input, "owner", 1).unwrap_err().code,
        "PREVIEW_GENERATION_CONFLICT"
    );
    let mut current = input;
    let v = m.snapshot(1);
    current.generation = v.generation;
    m.verify_visual_frame(&current, "owner", 1).unwrap();
    current.overlay_epoch = "99".into();
    assert_eq!(
        m.verify_visual_frame(&current, "owner", 1)
            .unwrap_err()
            .code,
        "PREVIEW_GENERATION_CONFLICT"
    );
}
