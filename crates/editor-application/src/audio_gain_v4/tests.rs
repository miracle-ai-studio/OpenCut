use super::*;
use crate::project_v4::{
    StoredProjectRevisionV4,
    public::{ProjectV4PublicResources, plan_public},
};
use opencut_contracts::{
    AssetKind, AssetVersion, CreateProject, FrameRate, Time, TimeRange, TransactionResult,
};
use std::cell::Cell;

fn t(n: i64, d: u32) -> Time {
    Time {
        ticks: n.to_string(),
        timebase: d,
    }
}
struct Fixture {
    current: ProjectSnapshotV4,
    authorizations: Cell<usize>,
    reads: Cell<usize>,
    deny_after: Option<usize>,
}
impl Fixture {
    fn new() -> Self {
        let mut project = crate::project_v4::new_document(&CreateProject {
            id: "project".into(),
            name: "Gain".into(),
            sequence_id: "scene".into(),
            width: 160,
            height: 90,
            fps: FrameRate {
                numerator: 30,
                denominator: 1,
            },
        })
        .unwrap();
        project.sequences[0].main_track_id = "main".into();
        project.sequences[0].tracks[0].id = "main".into();
        project.assets.push(AssetV4 {
            id: "asset".into(),
            name: "Offline audio".into(),
            kind: AssetKind::Audio,
            version: AssetVersion {
                id: format!("av_{}", "a".repeat(64)),
                content_hash: "a".repeat(64),
                size: 1,
            },
            duration: Some(t(10, 1)),
            origin: ResourceOriginV4::LegacyLocator {
                uri: "/missing-not-read.wav".into(),
            },
            metadata: MediaMetadataV4 {
                mime_type: None,
                intrinsic_size: None,
                frame_rate: None,
                rotation_degrees: None,
                audio: AudioPresenceV4::Present { metadata: None },
            },
            license: None,
        });
        let mut sound = project.sequences[0].tracks[0].clone();
        sound.id = "sound".into();
        sound.kind = TrackKindV4::Audio;
        sound.elements.push(TimelineElementV4 {
            id: "voice".into(),
            name: "Preserved name".into(),
            timeline_range: TimeRange {
                start: t(1, 3),
                duration: t(2, 1),
            },
            source_span: ElementSourceSpanV4::ClassicTrimmed {
                trim_start: t(1, 1),
                trim_end: t(6, 1),
                source_duration: Some(t(10, 1)),
            },
            enabled: true,
            content: ElementContentV4::Audio {
                asset_id: "asset".into(),
                audio: AudioStyleV4 {
                    stream: Some(2),
                    gain: GainV4::Decibels { value: -6.05 },
                    muted: false,
                },
                retime: RetimeV4 {
                    rate: RationalRateV4 {
                        numerator: 3,
                        denominator: 2,
                    },
                    maintain_pitch: true,
                    mapping: RetimeMappingV4::ClassicRoundedTicks { timebase: 120000 },
                },
            },
            visual: None,
            channels: vec![],
        });
        project.sequences[0].tracks.push(sound);
        opencut_editor_core::project_v4::validate_project_v4(&project).unwrap();
        Self {
            current: ProjectSnapshotV4 {
                project,
                revision: "fixed-head".into(),
            },
            authorizations: Cell::new(0),
            reads: Cell::new(0),
            deny_after: None,
        }
    }
    fn request(&self, command: AudioGainCommandV4) -> AudioGainRequestV4 {
        AudioGainRequestV4 {
            project_id: "project".into(),
            expected_revision: self.current.revision.clone(),
            sequence_id: "scene".into(),
            track_id: "sound".into(),
            element_id: "voice".into(),
            idempotency_key: "gain-key".into(),
            label: "Clip gain".into(),
            command,
        }
    }
    fn element(&self) -> &TimelineElementV4 {
        &self.current.project.sequences[0].tracks[1].elements[0]
    }
    fn element_mut(&mut self) -> &mut TimelineElementV4 {
        &mut self.current.project.sequences[0].tracks[1].elements[0]
    }
    fn audio_mut(&mut self) -> &mut AudioStyleV4 {
        match &mut self.element_mut().content {
            ElementContentV4::Audio { audio, .. } | ElementContentV4::Video { audio, .. } => audio,
            _ => panic!(),
        }
    }
    fn as_video(&mut self) {
        self.current.project.assets[0].kind = AssetKind::Video;
        self.current.project.sequences[0].tracks[1].kind = TrackKindV4::Video;
        let e = self.element_mut();
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
            video_stream: Some(1),
            source_audio_enabled: false,
        };
        e.visual = Some(VisualStyleV4 {
            profile: VisualProfileV4::ClassicCanvasV31,
            transform: TransformV4 {
                position_x_px: 8.0,
                position_y_px: -3.0,
                scale_x: -0.5,
                scale_y: 2.0,
                rotation_degrees: 23.0,
            },
            opacity: 0.75,
            blend_mode: BlendModeV4::Normal,
            effects: vec![],
            masks: vec![],
        });
    }
}
impl ProjectV4Read for Fixture {
    fn authorize_v4(&self, _: V4Intent<'_>) -> Result<(), EditorError> {
        let next = self.authorizations.get() + 1;
        self.authorizations.set(next);
        if self.deny_after.is_some_and(|allowed| next > allowed) {
            Err(EditorError::new("NOT_AUTHORIZED", "Revoked current caller"))
        } else {
            Ok(())
        }
    }
    fn head_v4_source(&self, _: &str) -> Result<StoredProjectRevisionV4, EditorError> {
        self.reads.set(self.reads.get() + 1);
        Ok(StoredProjectRevisionV4 {
            revision: self.current.revision.clone(),
            document: StoredProjectDocumentV4::V4(Box::new(self.current.project.clone())),
        })
    }
    fn revision_v4_source(&self, _: &str, _: &str) -> Result<StoredProjectRevisionV4, EditorError> {
        unreachable!("No resource rebinding or historical-head substitution")
    }
    fn replay_v4_create(
        &self,
        _: &CreateV4Project,
    ) -> Result<Option<ProjectSnapshotV4>, EditorError> {
        unreachable!()
    }
    fn replay_v4_copy(&self, _: &CopyV4Project) -> Result<Option<ProjectSnapshotV4>, EditorError> {
        unreachable!()
    }
    fn replay_v4_edit(&self, _: V4Intent<'_>) -> Result<Option<TransactionResult>, EditorError> {
        unreachable!("Read-only planner has no mutation receipt")
    }
}
impl ProjectV4PublicResources for Fixture {
    fn authorized_project_media_v4(
        &self,
        _: &str,
        _: &str,
        _: &AssetVersion,
    ) -> Result<AssetV4, EditorError> {
        unreachable!("Authored gain never acquires CAS media")
    }
}
fn gain_channel(linear: bool) -> AnimationChannelV4 {
    AnimationChannelV4::Scalar {
        id: "volume".into(),
        target: ChannelTargetV4::Element {
            element_id: "voice".into(),
        },
        binding: ScalarBindingV4::Property {
            parameter: if linear {
                ScalarParameterV4::GainLinearLegacy
            } else {
                ScalarParameterV4::GainDecibels
            },
        },
        keys: vec![
            ScalarKeyV4 {
                id: "key-a".into(),
                segment_id: Some("segment".into()),
                time: t(0, 1),
                value: if linear { 0.5 } else { -6.05 },
                left_handle: None,
                right_handle: Some(CurveHandleV4 {
                    dt: t(1, 3),
                    dv: 0.1,
                }),
                segment_to_next: ScalarSegmentV4::Bezier,
                tangent_mode: TangentModeV4::Broken,
            },
            ScalarKeyV4 {
                id: "key-b".into(),
                segment_id: None,
                time: t(2, 1),
                value: if linear { 0.8 } else { -3.0 },
                left_handle: Some(CurveHandleV4 {
                    dt: t(-1, 3),
                    dv: -0.1,
                }),
                right_handle: None,
                segment_to_next: ScalarSegmentV4::Linear,
                tangent_mode: TangentModeV4::Broken,
            },
        ],
        extrapolation: ExtrapolationV4 {
            before: ExtrapolationModeV4::Hold,
            after: ExtrapolationModeV4::Linear,
        },
    }
}
fn proposed(fixture: &Fixture, output: &AudioGainPlanV4) -> ProjectDocumentV4 {
    let transaction = output.transaction.as_ref().unwrap();
    let [
        PublicProjectOperationV4::ElementPut {
            sequence_id,
            track_id,
            element,
        },
    ] = transaction.operations.as_slice()
    else {
        panic!("Gain planning must emit one exact ElementPut")
    };
    let proposed = crate::project_v4::plan(
        &fixture.current,
        &ApplyV4Project {
            project_id: transaction.project_id.clone(),
            expected_revision: transaction.expected_revision.clone(),
            idempotency_key: transaction.idempotency_key.clone(),
            label: transaction.label.clone(),
            operations: vec![ProjectOperationV4::ElementPut {
                sequence_id: sequence_id.clone(),
                track_id: track_id.clone(),
                element: element.clone(),
            }],
        },
    )
    .unwrap();
    assert_eq!(
        plan_public(fixture, transaction).unwrap(),
        crate::project_v4::public::project(&ProjectSnapshotV4 {
            project: proposed.clone(),
            revision: fixture.current.revision.clone(),
        })
        .project
    );
    proposed
}
#[test]
fn tenths_db_bounds_change_only_authored_gain_and_public_transaction_is_unchanged() {
    let mut fixture = Fixture::new();
    fixture.current.project.sequences[0].tracks[1].muted = true;
    let mut track_curve = gain_channel(false);
    if let AnimationChannelV4::Scalar { target, .. } = &mut track_curve {
        *target = ChannelTargetV4::Track {
            track_id: "sound".into(),
        };
    }
    // Track automation belongs to sequence time and is preserved. Only an
    // element's own gain envelope suppresses its static clip-line edit.
    fixture.current.project.sequences[0].tracks[1]
        .channels
        .push(track_curve);
    let before = fixture.current.clone();
    for tenth in [-600, -1, 0, 200] {
        let request = fixture.request(AudioGainCommandV4::SetDecibels {
            decibels_tenths: tenth,
        });
        let output = plan(&fixture, &request).unwrap();
        assert_eq!(plan(&fixture, &request).unwrap(), output);
        assert_eq!(fixture.current, before);
        assert!(
            output.decision.track_muted
                && !output.decision.element_muted
                && !output.decision.animated
        );
        assert_eq!(output.decision.old_gain, GainV4::Decibels { value: -6.05 });
        let mut expected = before.project.clone();
        if let ElementContentV4::Audio { audio, .. } =
            &mut expected.sequences[0].tracks[1].elements[0].content
        {
            audio.gain = GainV4::Decibels {
                value: f64::from(tenth) / 10.0,
            };
        }
        assert_eq!(proposed(&fixture, &output), expected);
        assert_eq!(output.transaction.as_ref().unwrap().operations.len(), 1);
        let wire = serde_json::to_string(&output).unwrap();
        assert!(!wire.contains("/missing"));
        assert!(!wire.contains("\"uri\""));
        assert_eq!(
            serde_json::from_str::<AudioGainPlanV4>(&wire).unwrap(),
            output
        );
    }
    for tenth in [-601, 201, i32::MIN, i32::MAX] {
        assert_eq!(
            plan(
                &fixture,
                &fixture.request(AudioGainCommandV4::SetDecibels {
                    decibels_tenths: tenth
                })
            )
            .unwrap_err()
            .code,
            "INVALID_ARGUMENT"
        );
    }
}
#[test]
fn mute_is_independent_of_silence_floor_and_preserves_both_gain_animation_profiles() {
    for linear in [false, true] {
        let mut fixture = Fixture::new();
        if linear {
            fixture.audio_mut().gain = GainV4::LinearLegacy { value: 0.5 };
        }
        fixture.element_mut().channels.push(gain_channel(linear));
        let original = fixture.element().clone();
        let output = plan(
            &fixture,
            &fixture.request(AudioGainCommandV4::SetMuted { muted: true }),
        )
        .unwrap();
        assert!(
            output.decision.animated
                && output.decision.element_muted
                && !output.decision.old_element_muted
        );
        assert_eq!(output.decision.old_gain, output.decision.new_gain);
        let mut expected = original.clone();
        if let ElementContentV4::Audio { audio, .. } = &mut expected.content {
            audio.muted = true;
        }
        assert_eq!(
            proposed(&fixture, &output).sequences[0].tracks[1].elements[0],
            expected
        );
        assert_eq!(
            serde_json::to_vec(&expected.channels).unwrap(),
            serde_json::to_vec(&original.channels).unwrap()
        );
        let error = plan(
            &fixture,
            &fixture.request(AudioGainCommandV4::SetDecibels {
                decibels_tenths: -60,
            }),
        )
        .unwrap_err();
        assert_eq!(
            error.code,
            if linear {
                "UNSUPPORTED_GAIN_PROFILE"
            } else {
                "GAIN_ANIMATED"
            }
        );
    }
    let fixture = Fixture::new();
    let floor = plan(
        &fixture,
        &fixture.request(AudioGainCommandV4::SetDecibels {
            decibels_tenths: -600,
        }),
    )
    .unwrap();
    assert!(!floor.decision.element_muted);
    assert_eq!(floor.decision.new_gain, GainV4::Decibels { value: -60.0 });
}
#[test]
fn known_audio_video_extension_preserves_visual_stream_clock_and_source_switch() {
    let mut fixture = Fixture::new();
    fixture.as_video();
    let before = fixture.element().clone();
    let output = plan(
        &fixture,
        &fixture.request(AudioGainCommandV4::SetDecibels {
            decibels_tenths: -120,
        }),
    )
    .unwrap();
    let mut expected = before;
    if let ElementContentV4::Video { audio, .. } = &mut expected.content {
        audio.gain = GainV4::Decibels { value: -12.0 };
    }
    assert_eq!(
        proposed(&fixture, &output).sequences[0].tracks[1].elements[0],
        expected
    );
    for (presence, code) in [
        (AudioPresenceV4::Unknown {}, "MEDIA_METADATA_REQUIRED"),
        (AudioPresenceV4::Absent {}, "NO_AUDIO_TRACK"),
    ] {
        fixture.current.project.assets[0].metadata.audio = presence;
        for command in [
            AudioGainCommandV4::SetDecibels {
                decibels_tenths: -120,
            },
            AudioGainCommandV4::SetMuted { muted: true },
        ] {
            assert_eq!(
                plan(&fixture, &fixture.request(command)).unwrap_err().code,
                code
            );
        }
    }
    // Audio clip editing needs only the stored model, not a decoder/probe or CAS
    // availability. Unknown presence cannot turn an authored edit into I/O.
    let mut fixture = Fixture::new();
    fixture.current.project.assets[0].metadata.audio = AudioPresenceV4::Unknown {};
    assert!(
        plan(
            &fixture,
            &fixture.request(AudioGainCommandV4::SetMuted { muted: true })
        )
        .is_ok()
    );
}
#[test]
fn no_change_still_checks_locks_live_authority_and_exact_current_head() {
    let mut fixture = Fixture::new();
    fixture.audio_mut().gain = GainV4::Decibels { value: -6.0 };
    for command in [
        AudioGainCommandV4::SetDecibels {
            decibels_tenths: -60,
        },
        AudioGainCommandV4::SetMuted { muted: false },
    ] {
        let output = plan(&fixture, &fixture.request(command)).unwrap();
        assert!(output.transaction.is_none());
    }
    assert_eq!(fixture.authorizations.get(), 4);
    fixture.current.project.sequences[0].tracks[1].locked = true;
    assert_eq!(
        plan(
            &fixture,
            &fixture.request(AudioGainCommandV4::SetMuted { muted: false })
        )
        .unwrap_err()
        .code,
        "CONSTRAINT_VIOLATION"
    );
    let mut denied = Fixture::new();
    denied.deny_after = Some(0);
    assert_eq!(
        plan(
            &denied,
            &denied.request(AudioGainCommandV4::SetMuted { muted: true })
        )
        .unwrap_err()
        .code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(denied.reads.get(), 0);
    let mut revoked = Fixture::new();
    revoked.deny_after = Some(1);
    assert_eq!(
        plan(
            &revoked,
            &revoked.request(AudioGainCommandV4::SetMuted { muted: true })
        )
        .unwrap_err()
        .code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(revoked.reads.get(), 1);
    let fixture = Fixture::new();
    let mut stale = fixture.request(AudioGainCommandV4::SetMuted { muted: true });
    stale.expected_revision = "older".into();
    assert_eq!(
        plan(&fixture, &stale).unwrap_err().code,
        "REVISION_CONFLICT"
    );
    let mut misplaced = fixture.request(AudioGainCommandV4::SetMuted { muted: true });
    misplaced.track_id = "main".into();
    assert_eq!(plan(&fixture, &misplaced).unwrap_err().code, "NOT_FOUND");
}
#[test]
fn independent_pre_master_block_gain_distinguishes_minus_sixty_from_mute() {
    use opencut_composition::project_v4::{
        AudioExecutionProfileV4, PcmFormatV4, SampleGainV4, compile_audio_block_v4,
        prepare_audio_v4,
    };
    let fixture = Fixture::new();
    for (tenths, expected_sample) in [(-600, 0.00025), (0, 0.25), (200, 2.5)] {
        let output = plan(
            &fixture,
            &fixture.request(AudioGainCommandV4::SetDecibels {
                decibels_tenths: tenths,
            }),
        )
        .unwrap();
        let snapshot = ProjectSnapshotV4 {
            project: proposed(&fixture, &output),
            revision: "proposed-gain".into(),
        };
        let prepared = prepare_audio_v4(
            &snapshot,
            "scene",
            AudioExecutionProfileV4::ClassicV31,
            PcmFormatV4 {
                sample_rate: 44100,
                channels: 2,
            },
        )
        .unwrap();
        let block = compile_audio_block_v4(&prepared, 14700, 1).unwrap();
        let SampleGainV4::Constant { amplitude } = &block.voices[0].gain else {
            panic!("Static authored gain must compile to a constant")
        };
        assert!((0.25 * amplitude - expected_sample).abs() < 1e-12);
    }
    let output = plan(
        &fixture,
        &fixture.request(AudioGainCommandV4::SetMuted { muted: true }),
    )
    .unwrap();
    let snapshot = ProjectSnapshotV4 {
        project: proposed(&fixture, &output),
        revision: "proposed-mute".into(),
    };
    let prepared = prepare_audio_v4(
        &snapshot,
        "scene",
        AudioExecutionProfileV4::ClassicV31,
        PcmFormatV4 {
            sample_rate: 44100,
            channels: 2,
        },
    )
    .unwrap();
    assert!(
        compile_audio_block_v4(&prepared, 14700, 1)
            .unwrap()
            .voices
            .is_empty()
    );
}
