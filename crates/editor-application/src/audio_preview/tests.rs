use super::*;
use opencut_composition::scene::PreviewGainOverride;
use opencut_contracts::Time;

fn time(n: i64, d: u32) -> Time {
    Time {
        ticks: n.to_string(),
        timebase: d,
    }
}
fn audio_snapshot() -> opencut_contracts::scene::EditorSnapshot {
    use opencut_contracts::{
        AssetKind, AssetVersion, CreateProject, FrameRate, TimeRange, scene::*,
    };
    let mut project = crate::scene::new_document(&CreateProject {
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
    project.assets.push(ProjectAsset {
        id: "asset".into(),
        name: "Audio".into(),
        kind: AssetKind::Audio,
        version: AssetVersion {
            id: format!("av_{}", "a".repeat(64)),
            content_hash: "a".repeat(64),
            size: 16,
        },
        duration: Some(time(1001, 1)),
        origin: ResourceOrigin::LegacyLocator {
            uri: "/private/not-public.wav".into(),
        },
        license: None,
        metadata: ProjectMediaMetadata {
            mime_type: Some("audio/wav".into()),
            intrinsic_size: None,
            frame_rate: None,
            rotation_degrees: None,
            audio: AudioPresence::Present { metadata: None },
        },
    });
    project.sequences[0].tracks.push(SceneTrack {
        id: "sound".into(),
        name: "Sound".into(),
        kind: SceneTrackKind::Audio,
        enabled: true,
        muted: false,
        locked: false,
        opacity: 1.0,
        gain: Gain::Decibels { value: 0.0 },
        channels: vec![],
        elements: vec![TimelineElement {
            id: "voice".into(),
            name: "Voice".into(),
            timeline_range: TimeRange {
                start: time(0, 1),
                duration: time(1000, 1),
            },
            source_span: ElementSourceSpan::ClassicTrimmed {
                trim_start: time(0, 1),
                trim_end: time(0, 1),
                source_duration: None,
            },
            enabled: true,
            visual: None,
            channels: vec![],
            content: ElementContent::Audio {
                asset_id: "asset".into(),
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
                    maintain_pitch: false,
                    mapping: RetimeMapping::ClassicRoundedTicks { timebase: 120000 },
                },
            },
        }],
    });
    EditorSnapshot {
        project,
        revision: "fixed".into(),
    }
}
fn machine() -> AudioPreviewMachine {
    machine_with_frames(96000)
}
fn machine_with_frames(total_frames: u64) -> AudioPreviewMachine {
    AudioPreviewMachine::new(
        &AudioPreviewCreate {
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
        total_frames,
        0,
    )
    .unwrap()
}
fn control(
    m: &AudioPreviewMachine,
    key: &str,
    command: AudioPreviewCommand,
) -> AudioPreviewControl {
    let v = m.snapshot(0);
    AudioPreviewControl {
        session_id: v.session_id,
        generation: v.generation,
        owner_id: v.owner_id,
        lease_id: v.lease_id,
        control_id: key.into(),
        command,
    }
}
#[test]
fn recovery_receipt_reads_exact_control_without_mutation_or_lease_refresh() {
    let mut m = machine();
    let play = control(&m, "recovery-play", AudioPreviewCommand::Play);
    let before = m.snapshot(0);
    assert!(m.recover_control_receipt(&play).unwrap().is_none());
    assert_eq!(m.snapshot(0), before);
    let applied = m.control(&play, 1).unwrap().session;
    let stop = control(&m, "recovery-stop", AudioPreviewCommand::Stop);
    m.control(&stop, 2).unwrap();
    let stopped = m.snapshot(2);
    assert_eq!(m.recover_control_receipt(&play).unwrap(), Some(applied));
    assert_eq!(m.snapshot(2), stopped);
    let mut changed = play.clone();
    changed.command = AudioPreviewCommand::Seek {
        sequence_time: time(0, 1),
    };
    assert_eq!(
        m.recover_control_receipt(&changed).unwrap_err().code,
        "IDEMPOTENCY_KEY_REUSED"
    );
    assert_eq!(m.snapshot(2), stopped);
}
fn report(m: &mut AudioPreviewMachine, key: &str, count: u64, sequence: u64) {
    let input = control(
        m,
        key,
        AudioPreviewCommand::ReportConsumed {
            sequence: sequence.to_string(),
            consumed_frames: count.to_string(),
            output: Some(AudioPreviewOutputObservation {
                frames: count.to_string(),
                uncertainty_frames: Some(0),
                source: AudioPreviewClockSource::DeviceTimestamp,
            }),
            playing: false,
            stalled: false,
        },
    );
    m.control(&input, 10).unwrap();
}
#[test]
fn credit_and_final_consumption_are_distinct_exact_clocks() {
    let mut m = machine_with_frames(480000);
    assert_eq!(m.actual_start_sample(), 16000);
    let play = control(&m, "play", AudioPreviewCommand::Play);
    m.control(&play, 1).unwrap();
    for start in (16000..16000 + AUDIO_PREVIEW_MAX_BUFFERED_FRAMES).step_by(4096) {
        m.produced(m.generation(), m.overlay_epoch(), start, 4096, 2)
            .unwrap();
    }
    assert_eq!(m.next_block(3).unwrap(), None);
    assert_eq!(m.snapshot(3).consumed_frames, "0");
    report(&mut m, "consume", 1024, 0);
    assert_eq!(m.next_block(10).unwrap(), None);
    assert_eq!(m.snapshot(10).presentation_time, time(17024, 48000));
    let too_far = control(
        &m,
        "false-produced",
        AudioPreviewCommand::ReportConsumed {
            sequence: "1".into(),
            consumed_frames: (AUDIO_PREVIEW_MAX_BUFFERED_FRAMES + 1).to_string(),
            output: None,
            playing: true,
            stalled: false,
        },
    );
    let unchanged = m.snapshot(10);
    assert!(m.control(&too_far, 10).is_err());
    assert_eq!(m.snapshot(10), unchanged);
    report(&mut m, "consume-first-block", 4096, 1);
    assert_eq!(m.next_block(10).unwrap(), Some((16000 + AUDIO_PREVIEW_MAX_BUFFERED_FRAMES, 4096)));
    assert!(m.disconnect(m.generation(), 11).unwrap());
    assert_eq!(m.snapshot(11).state, AudioPreviewState::Paused);
    report(&mut m, "final", 5120, 2);
    let resume = control(&m, "resume", AudioPreviewCommand::Play);
    m.control(&resume, 12).unwrap();
    assert_eq!(m.actual_start_sample(), 21120);
    assert_eq!(m.snapshot(12).consumed_frames, "0");
}
#[test]
fn submitted_credit_is_not_output_and_pause_uses_the_device_witness() {
    let mut m = machine();
    let play = control(&m, "start-output", AudioPreviewCommand::Play);
    m.control(&play, 1).unwrap();
    m.produced(m.generation(), m.overlay_epoch(), 16000, 4096, 2)
        .unwrap();
    let submit = control(
        &m,
        "submitted",
        AudioPreviewCommand::ReportConsumed {
            sequence: "1".into(),
            consumed_frames: "4096".into(),
            playing: true,
            stalled: false,
            output: None,
        },
    );
    m.control(&submit, 3).unwrap();
    let view = m.snapshot(3);
    assert_eq!(view.submitted_time, time(20096, 48000));
    assert_eq!(view.presentation_time, time(16000, 48000));
    assert!(!view.presentation_fresh);
    let witness = control(
        &m,
        "output",
        AudioPreviewCommand::ReportConsumed {
            sequence: "2".into(),
            consumed_frames: "4096".into(),
            playing: true,
            stalled: false,
            output: Some(AudioPreviewOutputObservation {
                frames: "1024".into(),
                source: AudioPreviewClockSource::DeviceTimestamp,
                uncertainty_frames: None,
            }),
        },
    );
    m.control(&witness, 4).unwrap();
    assert_eq!(m.snapshot(4).presentation_time, time(17024, 48000));
    let pause = control(&m, "pause-output", AudioPreviewCommand::Pause);
    m.control(&pause, 5).unwrap();
    assert_eq!(m.actual_start_sample(), 17024);
    assert_eq!(m.snapshot(5).consumed_frames, "0");
    assert!(m.snapshot(5).output.is_none());
}
#[test]
fn device_output_cannot_pass_submission_or_move_backwards() {
    let mut m = machine();
    let play = control(&m, "start", AudioPreviewCommand::Play);
    m.control(&play, 1).unwrap();
    m.produced(m.generation(), m.overlay_epoch(), 16000, 4096, 2)
        .unwrap();
    report(&mut m, "observed", 2048, 1);
    for (key, frames) in [("ahead", "4097"), ("backwards", "2047")] {
        let input = control(
            &m,
            key,
            AudioPreviewCommand::ReportConsumed {
                sequence: "2".into(),
                consumed_frames: "4096".into(),
                playing: true,
                stalled: false,
                output: Some(AudioPreviewOutputObservation {
                    frames: frames.into(),
                    source: AudioPreviewClockSource::DeviceTimestamp,
                    uncertainty_frames: None,
                }),
            },
        );
        let old = m.snapshot(11);
        assert!(m.control(&input, 11).is_err());
        assert_eq!(m.snapshot(11), old);
    }
}
#[test]
fn submitting_the_last_sample_does_not_end_before_device_output() {
    let mut m = machine();
    let play = control(&m, "start", AudioPreviewCommand::Play);
    m.control(&play, 1).unwrap();
    let mut count = 0;
    while let Some((start, frames)) = m.next_block(2).unwrap() {
        m.produced(m.generation(), m.overlay_epoch(), start, frames, 2)
            .unwrap();
        count += u64::from(frames);
        let input = control(
            &m,
            &format!("submit-{count}"),
            AudioPreviewCommand::ReportConsumed {
                sequence: count.to_string(),
                consumed_frames: count.to_string(),
                playing: true,
                stalled: false,
                output: None,
            },
        );
        m.control(&input, 3).unwrap();
    }
    assert_eq!(m.snapshot(3).state, AudioPreviewState::Playing);
    report(&mut m, "drained", count, count + 1);
    assert_eq!(m.snapshot(10).state, AudioPreviewState::Ended);
}
#[test]
fn validated_overlay_reanchors_after_report_and_replay_never_restarts() {
    let mut m = machine();
    let play = control(&m, "play", AudioPreviewCommand::Play);
    m.control(&play, 1).unwrap();
    m.produced(m.generation(), 0, 16000, 1000, 2).unwrap();
    report(&mut m, "consume", 731, 0);
    let old = m.generation();
    let gain = TemporaryAudioGain {
        track_id: "t".into(),
        element_id: "e".into(),
        decibels_tenths: -60,
        gesture_id: "gesture".into(),
        update_sequence: "1".into(),
    };
    let input = control(
        &m,
        "gain",
        AudioPreviewCommand::SetTemporaryGain { gain: gain.clone() },
    );
    assert!(m.control(&input, 11).is_err());
    assert_eq!(m.generation(), old);
    let valid = PreviewGainOverride {
        track_id: gain.track_id.clone(),
        element_id: gain.element_id.clone(),
        decibels_tenths: -60,
    };
    let response = m.control_validated(&input, Some(&valid), 11).unwrap();
    assert_eq!(response.effect, AudioPreviewEffect::Replace);
    assert_eq!(response.session.state, AudioPreviewState::Playing);
    assert_eq!(m.actual_start_sample(), 16731);
    assert_eq!(m.overlay_epoch(), 1);
    let replay = m.control(&input, 12).unwrap();
    assert_eq!(replay.effect, AudioPreviewEffect::None);
    assert_eq!(replay.session, response.session);
    assert_eq!(
        m.produced(old, 0, 17000, 2, 12).unwrap_err().code,
        "PREVIEW_GENERATION_CONFLICT"
    );
    let mut changed = input.clone();
    changed.command = AudioPreviewCommand::Stop;
    assert_eq!(
        m.control(&changed, 12).unwrap_err().code,
        "IDEMPOTENCY_KEY_REUSED"
    );
    let clear = control(
        &m,
        "clear",
        AudioPreviewCommand::ClearTemporaryGain {
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
    let play = control(&m, "play", AudioPreviewCommand::Play);
    m.control(&play, 1).unwrap();
    let old = m.snapshot(2);
    let seek = control(
        &m,
        "bad",
        AudioPreviewCommand::Seek {
            sequence_time: time(2, 1),
        },
    );
    assert!(m.control(&seek, 2).is_err());
    assert_eq!(m.snapshot(2), old);
    let mut foreign = control(&m, "foreign", AudioPreviewCommand::Stop);
    foreign.owner_id = "other".into();
    assert_eq!(m.control(&foreign, 2).unwrap_err().code, "NOT_AUTHORIZED");
    assert!(m.expire(30000).unwrap());
    assert_eq!(m.snapshot(30000).state, AudioPreviewState::Stopped);
    let acquire = AudioPreviewAcquire {
        session_id: "session".into(),
        expected_generation: m.generation().to_string(),
        control_id: "takeover".into(),
    };
    let next = m
        .acquire(&acquire, "new-owner".into(), "new-lease".into(), 30001)
        .unwrap();
    assert_eq!(next.session.state, AudioPreviewState::Paused);
    assert_eq!(next.session.identity.revision, "fixed");
    let stop = control(&m, "stop", AudioPreviewCommand::Stop);
    m.control(&stop, 30002).unwrap();
    let take = AudioPreviewAcquire {
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
        AudioPreviewCommand::Seek {
            sequence_time: time(1, 7),
        },
    );
    let view = m.control(&input, 1).unwrap().session;
    assert_eq!(view.actual_start_sample, "6857");
    assert_eq!(view.presentation_time, time(6857, 48000));
    assert_eq!(m.next_block(2).unwrap(), None);
    let json = serde_json::to_value(view).unwrap();
    assert!(json.get("paths").is_none());
    assert_eq!(json["identity"]["profile"], "classic-realtime-v31");
}
#[test]
fn preview_admits_actual_native_pcm_without_whole_scene_output_and_refuses_unverified_downmix() {
    use crate::classic_audio::*;
    use opencut_composition::scene::*;
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
    let p = PreparedPreviewAudio::new(
        &audio_snapshot(),
        "s",
        PcmFormat {
            sample_rate: 48000,
            channels: 2,
        },
    )
    .unwrap();
    assert!(ClassicAudioSession::new(p.audio(), 1024 * 1024, 1024).is_err());
    let mut session = ClassicAudioSession::new_preview(&p, AUDIO_PREVIEW_MAX_BUFFERED_FRAMES * 8 + 1024 * 1024, 1024).unwrap();
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
    let format = PreviewSourceFormat {
        element_id: "voice".into(),
        sample_rate: 44100,
        frame_count: 1024,
        channels: 1,
        prepared: false,
        segments: vec![PreviewAudioSegment {
            segment_id: 3,
            timestamp_seconds: 0.0,
            frame_count: 1024,
        }],
    };
    session
        .callback(
            AudioCallback::BindPreview(BindPreviewSources {
                window: None,
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
    let mut changed_format = format.clone();
    changed_format.segments[0].segment_id = 4;
    assert!(
        session
            .callback(
                AudioCallback::BindPreview(BindPreviewSources {
                    window: None,
                    source_formats: vec![changed_format],
                }),
                &mut resources
            )
            .is_err()
    );
    // Rejected rebinding must leave the original compiled PTS index usable;
    // exact idempotent rebinding remains accepted.
    session
        .callback(
            AudioCallback::BindPreview(BindPreviewSources {
                window: None,
                source_formats: vec![format.clone()],
            }),
            &mut resources,
        )
        .unwrap();
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
            profile: CLASSIC_REALTIME_AUDIO_PROFILE.into(),
            sample_rate: 48000,
            channels: 2,
        },
        engine_identity: "native-verified-runtime".into(),
        decoders: vec![ClassicAudioPreviewDecoderReceipt {
            asset_id: "asset".into(),
            primary,
            sample_rate: 44100,
            frame_count: Some("1024".into()),
            channels: 1,
        }],
        source_formats: vec![format],
        preparation: vec![],
        peak_accounted_bytes: AUDIO_PREVIEW_MAX_BUFFERED_FRAMES * 8 + 1024 * 4,
    };
    session.validate_preview_receipt(&receipt).unwrap();
    receipt.decoders[0].primary.track_id = 1;
    assert!(session.validate_preview_receipt(&receipt).is_err());
}

#[test]
fn picture_token_preserves_owner_expiry_and_generation_without_opening_ready_pcm() {
    let mut m = machine();
    let v = m.snapshot(0);
    let input = AudioPreviewStream {
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
            AudioPreviewCommand::Seek {
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

#[test]
fn rolling_preview_counts_residency_and_preserves_command_primary_and_policy_fences() {
    use crate::classic_audio::*;
    use opencut_composition::scene::{PcmFormat, PreparedPreviewAudio};
    use opencut_contracts::{EditorError, scene::ElementContent};
    struct CheckpointOnly {
        revoked: bool,
    }
    impl ClassicAudioResources for CheckpointOnly {
        fn resolve(&mut self, _: &AudioResource, _: &str) -> Result<(String, u64), EditorError> {
            panic!("admission must not resolve native bytes")
        }
        fn read(&mut self, _: &str, _: u64, _: u32) -> Result<(Vec<u8>, bool), EditorError> {
            panic!("admission must not read native bytes")
        }
        fn release(&mut self, _: &str) -> Result<(), EditorError> {
            panic!("admission must not own native transfers")
        }
        fn checkpoint(&mut self) -> Result<(), EditorError> {
            if self.revoked {
                Err(EditorError::new("NOT_AUTHORIZED", "fixture revoked"))
            } else {
                Ok(())
            }
        }
    }
    let mut snapshot = audio_snapshot();
    snapshot.project.assets[0].version.size = 7_805_484;
    snapshot.project.assets[0].duration = Some(time(300, 1));
    snapshot.project.sequences[0]
        .tracks
        .last_mut()
        .unwrap()
        .elements[0]
        .timeline_range
        .duration = time(300, 1);
    let prepared = PreparedPreviewAudio::new(
        &snapshot,
        "s",
        PcmFormat {
            sample_rate: 48000,
            channels: 2,
        },
    )
    .unwrap();
    let mut session =
        ClassicAudioSession::new_preview(&prepared, 256 * 1024 * 1024, 32 * 1024 * 1024).unwrap();
    let request = DecodeAdmission {
        resource: session.resource("asset").unwrap(),
        primary: Some(ClassicAudioPrimarySelection {
            track_id: 1,
            codec: "aac".into(),
            sample_rate: 44100,
            channels: 2,
        }),
    };
    let mut resources = CheckpointOnly { revoked: false };
    let admission = session
        .callback(
            AudioCallback::AdmitPreviewDecode(request.clone()),
            &mut resources,
        )
        .unwrap();
    assert!(admission["preparation"].is_null());
    let frames = admission["stream"]["workingFrames"].as_u64().unwrap();
    let chunk = admission["stream"]["maxChunkFrames"].as_u64().unwrap();
    assert!(frames >= 2 * chunk && chunk <= 65536);
    assert!(
        (frames + 4 * chunk) * 2 * 4 + 3 * 7_805_484 + AUDIO_PREVIEW_MAX_BUFFERED_FRAMES * 8
            <= 256 * 1024 * 1024
    );
    assert_eq!(
        session
            .callback(
                AudioCallback::AdmitPreviewDecode(request.clone()),
                &mut resources
            )
            .unwrap(),
        admission
    );
    assert_eq!(
        session
            .callback(AudioCallback::Admit(request.clone()), &mut resources)
            .unwrap_err()
            .code,
        "INVALID_AUDIO_PLAN"
    );
    let mut changed = request.clone();
    changed.primary.as_mut().unwrap().track_id = 2;
    assert_eq!(
        session
            .callback(AudioCallback::AdmitPreviewDecode(changed), &mut resources)
            .unwrap_err()
            .code,
        "INVALID_AUDIO_PLAN"
    );
    resources.revoked = true;
    assert_eq!(
        session
            .callback(
                AudioCallback::AdmitPreviewDecode(request.clone()),
                &mut resources
            )
            .unwrap_err()
            .code,
        "NOT_AUTHORIZED"
    );
    resources.revoked = false;
    session
        .callback(
            AudioCallback::ObservePreviewDecoded(PreviewDecodedObservation {
                resource: request.resource.clone(),
                sample_rate: 44100,
                frame_count: "0".into(),
                channels: 2,
            }),
            &mut resources,
        )
        .unwrap();
    assert_eq!(
        session
            .callback(
                AudioCallback::ObservePreviewDecoded(PreviewDecodedObservation {
                    resource: request.resource,
                    sample_rate: 44100,
                    frame_count: (frames + 1).to_string(),
                    channels: 2
                }),
                &mut resources
            )
            .unwrap_err()
            .code,
        "INVALID_AUDIO_PLAN"
    );
    // Reuse the same long-source fixture. Header readiness admits no fabricated
    // complete PCM; each compile must use the exact Rust-planned current window.
    let request = PreviewBlockRequest {
        start_sample: "0".into(),
        frame_count: 7,
    };
    let windows = session
        .callback(
            AudioCallback::PreviewWindows(request.clone()),
            &mut resources,
        )
        .unwrap();
    assert_eq!(windows[0]["assetId"], "asset");
    assert_eq!(
        session
            .callback(AudioCallback::PreviewBlock(request.clone()), &mut resources)
            .unwrap_err()
            .code,
        "INVALID_AUDIO_PLAN"
    );
    let formats = vec![opencut_composition::scene::PreviewSourceFormat {
        element_id: "voice".into(),
        sample_rate: 44100,
        frame_count: 32,
        channels: 2,
        prepared: false,
        segments: vec![opencut_composition::scene::PreviewAudioSegment {
            segment_id: 0,
            timestamp_seconds: 0.0,
            frame_count: 32,
        }],
    }];
    let wrong = PreviewBlockRequest {
        start_sample: "1".into(),
        frame_count: 7,
    };
    assert_eq!(
        session
            .callback(
                AudioCallback::BindPreview(BindPreviewSources {
                    source_formats: formats.clone(),
                    window: Some(wrong)
                }),
                &mut resources
            )
            .unwrap_err()
            .code,
        "INVALID_AUDIO_PLAN"
    );
    session
        .callback(
            AudioCallback::BindPreview(BindPreviewSources {
                source_formats: formats,
                window: Some(request.clone()),
            }),
            &mut resources,
        )
        .unwrap();
    let block = session
        .callback(AudioCallback::PreviewBlock(request.clone()), &mut resources)
        .unwrap();
    assert_eq!(block["voices"][0]["positions"][0]["lower"], "0");
    session.set_preview_state(100, None).unwrap();
    assert_eq!(
        session
            .callback(AudioCallback::PreviewBlock(request), &mut resources)
            .unwrap_err()
            .code,
        "INVALID_AUDIO_PLAN"
    );
    let voice = &mut snapshot.project.sequences[0]
        .tracks
        .last_mut()
        .unwrap()
        .elements[0];
    voice.timeline_range.duration = time(240, 1);
    if let ElementContent::Audio { retime, .. } = &mut voice.content {
        retime.maintain_pitch = true;
        retime.rate.numerator = 5;
        retime.rate.denominator = 4;
    }
    let prepared = PreparedPreviewAudio::new(
        &snapshot,
        "s",
        PcmFormat {
            sample_rate: 48000,
            channels: 2,
        },
    )
    .unwrap();
    let mut session =
        ClassicAudioSession::new_preview(&prepared, 256 * 1024 * 1024, 32 * 1024 * 1024).unwrap();
    let request = DecodeAdmission {
        resource: session.resource("asset").unwrap(),
        primary: Some(ClassicAudioPrimarySelection {
            track_id: 1,
            codec: "aac".into(),
            sample_rate: 44100,
            channels: 2,
        }),
    };
    assert_eq!(
        session
            .callback(AudioCallback::AdmitPreviewDecode(request), &mut resources)
            .unwrap_err()
            .code,
        "AUDIO_BUDGET_EXCEEDED"
    );
    // The same resource can feed an ordinary rolling voice and two independent
    // prepared outputs. Retire only the concat input,after both observations.
    snapshot.project.assets[0].duration = Some(time(2, 1));
    let track = snapshot.project.sequences[0].tracks.last_mut().unwrap();
    track.elements[0].timeline_range.duration = time(1, 1);
    let mut second = track.elements[0].clone();
    second.id = "prepared-second".into();
    let mut ordinary = track.elements[0].clone();
    ordinary.id = "ordinary".into();
    if let ElementContent::Audio { retime, .. } = &mut ordinary.content {
        retime.maintain_pitch = false;
    }
    // Independent simultaneous voices occupy independent tracks. Overlapping
    // elements on one track are rejected before the residency protocol starts.
    let mut second_track = track.clone();
    second_track.id = "prepared-second-track".into();
    second_track.elements = vec![second];
    let mut ordinary_track = track.clone();
    ordinary_track.id = "ordinary-track".into();
    ordinary_track.elements = vec![ordinary];
    snapshot.project.sequences[0]
        .tracks
        .extend([second_track, ordinary_track]);
    let prepared = PreparedPreviewAudio::new(
        &snapshot,
        "s",
        PcmFormat {
            sample_rate: 48000,
            channels: 2,
        },
    )
    .unwrap();
    let mut session =
        ClassicAudioSession::new_preview(&prepared, 256 * 1024 * 1024, 32 * 1024 * 1024).unwrap();
    let resource = session.resource("asset").unwrap();
    let request = DecodeAdmission {
        resource: resource.clone(),
        primary: Some(ClassicAudioPrimarySelection {
            track_id: 1,
            codec: "aac".into(),
            sample_rate: 44100,
            channels: 2,
        }),
    };
    let allocation = session
        .callback(
            AudioCallback::AdmitPreviewDecode(request.clone()),
            &mut resources,
        )
        .unwrap();
    assert!(allocation["stream"].is_object() && allocation["preparation"].is_object());
    session
        .callback(
            AudioCallback::ObserveWindowedPreview(WindowedPreviewObservation {
                decoded: PreviewDecodedObservation {
                    resource: resource.clone(),
                    sample_rate: 44100,
                    frame_count: "1024".into(),
                    channels: 2,
                },
                resident_frame_count: "1115".into(),
            }),
            &mut resources,
        )
        .unwrap();
    let pitch = |id: &str| PitchRequest {
        element_id: id.into(),
        decoded_sample_rate: 48000,
        decoded_frame_count: "1115".into(),
        decoded_channels: 2,
        resident_frame_count: Some("1115".into()),
    };
    session
        .callback(AudioCallback::Pitch(pitch("voice")), &mut resources)
        .unwrap();
    assert!(
        session
            .callback(
                AudioCallback::ReleaseDecoded(DecodedRelease {
                    resource: resource.clone()
                }),
                &mut resources
            )
            .is_err()
    );
    session
        .callback(
            AudioCallback::Pitch(pitch("prepared-second")),
            &mut resources,
        )
        .unwrap();
    resources.revoked = true;
    assert_eq!(
        session
            .callback(
                AudioCallback::ReleaseDecoded(DecodedRelease {
                    resource: resource.clone()
                }),
                &mut resources
            )
            .unwrap_err()
            .code,
        "NOT_AUTHORIZED"
    );
    resources.revoked = false;
    session
        .callback(
            AudioCallback::ReleaseDecoded(DecodedRelease {
                resource: resource.clone(),
            }),
            &mut resources,
        )
        .unwrap();
    // Idempotent retirement retains original format facts for receipt checks.
    session
        .callback(
            AudioCallback::ReleaseDecoded(DecodedRelease {
                resource: resource.clone(),
            }),
            &mut resources,
        )
        .unwrap();
    assert!(
        session
            .callback(AudioCallback::Pitch(pitch("voice")), &mut resources)
            .is_err()
    );
    assert!(
        session
            .callback(AudioCallback::AdmitPreviewDecode(request), &mut resources)
            .is_err()
    );
    let windows = session
        .callback(
            AudioCallback::PreviewWindows(PreviewBlockRequest {
                start_sample: "0".into(),
                frame_count: 7,
            }),
            &mut resources,
        )
        .unwrap();
    assert_eq!(windows[0]["assetId"], "asset");
}
