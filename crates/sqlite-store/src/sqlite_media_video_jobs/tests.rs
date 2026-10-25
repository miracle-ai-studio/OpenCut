//! Queue/publication fixtures use real tiny CAS bytes, not DSP output acceptance.
#[path = "mask_tests.rs"]
mod mask_tests;
use super::*;
#[path = "blend_tests.rs"]
mod blend_tests;
#[path = "graphic_tests.rs"]
mod graphic_tests;
use opencut_contracts::{
    Asset, AssetKind, CreateProject, FrameRate, Time, TimeRange, scene::*, scene_ops::*,
};
use opencut_editor_application::access::{Caller, PrincipalPolicy, ProjectGrant};
use std::collections::BTreeMap;

fn time(ticks: i64, timebase: u32) -> Time {
    Time {
        ticks: ticks.to_string(),
        timebase,
    }
}
fn now() -> i64 {
    lifecycle::JobClock::now_ms(&sqlite_jobs::SystemClock).unwrap()
}
struct Fixture {
    app: EditorApplication,
    directory: tempfile::TempDir,
    revision: String,
}
impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut app = EditorApplication::open(directory.path().join("workspace.sqlite")).unwrap();
        let initial = app
            .create_scene_project(&CreateProjectRequest {
                project: CreateProject {
                    id: "audio-project".into(),
                    name: "Queued audio".into(),
                    sequence_id: "scene".into(),
                    width: 64,
                    height: 48,
                    fps: FrameRate {
                        numerator: 30,
                        denominator: 1,
                    },
                },
                idempotency_key: "create".into(),
            })
            .unwrap();
        // Valid tiny mono PCM16 WAV. Timeline zero-tail behavior is not rendered
        // by this fixture: it verifies immutable byte capture and SQL fences.
        let source = directory.path().join("tiny.wav");
        let mut bytes = b"RIFF".to_vec();
        bytes.extend(68u32.to_le_bytes());
        bytes.extend(b"WAVEfmt ");
        bytes.extend(16u32.to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
        bytes.extend(48000u32.to_le_bytes());
        bytes.extend(96000u32.to_le_bytes());
        bytes.extend(2u16.to_le_bytes());
        bytes.extend(16u16.to_le_bytes());
        bytes.extend(b"data");
        bytes.extend(32u32.to_le_bytes());
        bytes.extend([0; 32]);
        std::fs::write(&source, &bytes).unwrap();
        let observed = Asset {
            id: "audio-asset".into(),
            name: "Tiny WAV".into(),
            kind: AssetKind::Audio,
            uri: source.to_string_lossy().into(),
            duration: None,
            version: None,
            metadata: BTreeMap::new(),
        };
        let pinned = opencut_asset_store::AssetStore::open(app.asset_store_root())
            .unwrap()
            .pin_asset(&observed)
            .unwrap();
        let asset = ProjectAsset {
            id: observed.id,
            name: observed.name,
            kind: AssetKind::Audio,
            version: pinned.version.unwrap(),
            duration: None,
            origin: ResourceOrigin::LegacyLocator {
                uri: source.to_string_lossy().into(),
            },
            metadata: ProjectMediaMetadata {
                mime_type: Some("audio/wav".into()),
                intrinsic_size: None,
                frame_rate: None,
                rotation_degrees: None,
                audio: AudioPresence::Present {
                    metadata: Some(AudioMetadata {
                        sample_rate: 48000,
                        channels: 1,
                        channel_layout: None,
                    }),
                },
            },
            license: None,
        };
        let track = SceneTrack {
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
                enabled: true,
                timeline_range: TimeRange {
                    start: time(0, 1),
                    duration: time(1, 10),
                },
                source_span: ElementSourceSpan::ClassicTrimmed {
                    trim_start: time(0, 1),
                    trim_end: time(0, 1),
                    source_duration: None,
                },
                content: ElementContent::Audio {
                    asset_id: asset.id.clone(),
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
                visual: None,
                channels: vec![],
            }],
        };
        app.apply_scene_project(&ApplySceneProject {
            project_id: "audio-project".into(),
            expected_revision: initial.revision,
            idempotency_key: "bind-audio".into(),
            label: "Bind exact CAS audio".into(),
            operations: vec![
                ProjectOperation::AssetPut { asset },
                ProjectOperation::TrackPut {
                    sequence_id: "scene".into(),
                    track,
                },
            ],
        })
        .unwrap();
        let fixed = app
            .inspect_scene_project(&InspectProject {
                project_id: "audio-project".into(),
                revision: None,
            })
            .unwrap();
        Self {
            app,
            directory,
            revision: fixed.revision,
        }
    }
    fn request(&self, key: &str) -> FixedVideoInput {
        FixedVideoInput {
            idempotency_key: key.into(),
            project_id: "audio-project".into(),
            revision: self.revision.clone(),
            sequence_id: "scene".into(),
        }
    }
    fn submit(&mut self, key: &str) -> JobRecord {
        let request = self.request(key);
        start_video_mp4(&mut self.app, request).unwrap()
    }
    fn claim(&mut self, job: &JobRecord) -> JobRecord {
        self.app.set_caller(Caller::LocalOwner);
        let running = self
            .app
            .claim_job("frame-worker", now(), 30_000)
            .unwrap()
            .unwrap();
        assert_eq!(running.job_id, job.job_id);
        running
    }
    fn set_policy(&mut self, revision: &str, actions: Vec<ProjectAction>) {
        self.app
            .set_principal_policy(PrincipalPolicy {
                principal_id: "frame-agent".into(),
                expected_revision: revision.into(),
                enabled: true,
                grants: if actions.is_empty() {
                    vec![]
                } else {
                    vec![ProjectGrant {
                        project_id: "audio-project".into(),
                        actions,
                    }]
                },
            })
            .unwrap();
    }
    fn credential(&mut self) -> (Caller, String) {
        self.set_policy("0", vec![ProjectAction::Read, ProjectAction::RunJobs]);
        let issued = self.app.issue_credential("frame-agent").unwrap();
        (
            self.app.authenticate(&issued.secret).unwrap(),
            issued.credential_id,
        )
    }
}
fn counts(app: &EditorApplication) -> Vec<i64> {
    [
        "jobs",
        "job_projects",
        "idempotency",
        "job_events",
        "outbox",
        "audit_records",
    ]
    .into_iter()
    .map(|table| {
        app.connection
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    })
    .collect()
}
#[test]
fn frame_acceptance_is_fixed_atomic_idempotent_and_reopens_without_tracking_head() {
    let mut fixture = Fixture::new();
    let job = fixture.submit("fixed");
    let captured: PreparedMediaVideoMp4 = serde_json::from_value(job.input.clone()).unwrap();
    assert_eq!(captured.snapshot.revision, fixture.revision);
    assert_eq!(captured.asset_lock.assets.len(), 1);
    fixture
        .app
        .apply_scene_project(&ApplySceneProject {
            project_id: "audio-project".into(),
            expected_revision: fixture.revision.clone(),
            idempotency_key: "rename".into(),
            label: "Rename after fixed frame acceptance".into(),
            operations: vec![ProjectOperation::Rename {
                name: "Later head".into(),
            }],
        })
        .unwrap();
    assert_eq!(fixture.submit("fixed"), job);
    let mut changed = fixture.request("fixed");
    changed.sequence_id = "changed-scene".into();
    assert!(start_video_mp4(&mut fixture.app, changed).is_err());
    let reopened =
        EditorApplication::open(fixture.directory.path().join("workspace.sqlite")).unwrap();
    assert_eq!(reopened.get_job(&job.job_id).unwrap(), job);
    let view = reopened.job_view(&job.job_id).unwrap();
    assert_eq!(view.target.unwrap().revision, captured.snapshot.revision);
    assert!(view.artifacts.is_empty());
}
#[test]
fn frame_original_scope_and_credential_are_rechecked_by_owner_worker_and_sql_completion() {
    for missing in ["credential", "read", "run"] {
        let mut fixture = Fixture::new();
        let (caller, credential) = fixture.credential();
        fixture.app.set_caller(caller);
        let job = fixture.submit("scope");
        let running = fixture.claim(&job);
        let input: PreparedMediaVideoMp4 = serde_json::from_value(running.input.clone()).unwrap();
        fixture
            .app
            .revalidate_scene_video_job(
                &job.job_id,
                "frame-worker",
                running.attempt,
                &input.authority,
            )
            .unwrap();
        assert_eq!(
            fixture
                .app
                .revalidate_scene_video_job(
                    &job.job_id,
                    "frame-worker",
                    running.attempt,
                    &TrustedJobAuthority::LocalOwner
                )
                .unwrap_err()
                .code,
            "NOT_AUTHORIZED"
        );
        match missing {
            "credential" => fixture.app.revoke_credential(&credential).unwrap(),
            "read" => fixture.set_policy("1", vec![]),
            _ => fixture.set_policy("1", vec![ProjectAction::Read]),
        }
        assert_eq!(
            fixture
                .app
                .revalidate_scene_video_job(
                    &job.job_id,
                    "frame-worker",
                    running.attempt,
                    &input.authority
                )
                .unwrap_err()
                .code,
            "NOT_AUTHORIZED"
        );
        let before = counts(&fixture.app);
        assert_eq!(
            fixture
                .app
                .complete_job(
                    &job.job_id,
                    "frame-worker",
                    running.attempt,
                    now(),
                    Value::Null
                )
                .unwrap_err()
                .code,
            "NOT_AUTHORIZED"
        );
        assert_eq!(fixture.app.get_job(&job.job_id).unwrap(), running);
        assert_eq!(counts(&fixture.app), before);
    }
}
#[test]
fn frame_cancellation_wins_late_completion_and_failure_and_stale_attempt_is_refused() {
    for fail in [false, true] {
        let mut fixture = Fixture::new();
        let job = fixture.submit("cancel");
        let running = fixture.claim(&job);
        let input: PreparedMediaVideoMp4 = serde_json::from_value(running.input.clone()).unwrap();
        assert_eq!(
            fixture
                .app
                .revalidate_scene_video_job(
                    &job.job_id,
                    "frame-worker",
                    running.attempt + 1,
                    &input.authority
                )
                .unwrap_err()
                .code,
            "JOB_LEASE_LOST"
        );
        fixture.app.cancel_job(&job.job_id).unwrap();
        let done = if fail {
            fixture.app.fail_job(
                &job.job_id,
                "frame-worker",
                running.attempt,
                now(),
                json!({"code":"LATE"}),
            )
        } else {
            fixture.app.complete_job(
                &job.job_id,
                "frame-worker",
                running.attempt,
                now(),
                Value::Null,
            )
        }
        .unwrap();
        assert_eq!(done.state, JobState::Cancelled);
        assert!(done.result.is_none());
        assert!(
            fixture
                .app
                .job_view(&job.job_id)
                .unwrap()
                .artifacts
                .is_empty()
        );
        assert_eq!(fixture.submit("cancel").job_id, job.job_id);
    }
}
#[test]
fn frame_admission_faults_or_forged_private_policy_leave_no_partial_job_receipt() {
    let mut fixture = Fixture::new();
    fixture.app.connection.execute_batch("CREATE TRIGGER reject_frame_audit BEFORE INSERT ON audit_records WHEN NEW.action='job.accepted' BEGIN SELECT RAISE(ABORT,'injected failure'); END;").unwrap();
    let before = counts(&fixture.app);
    let request = fixture.request("fault");
    assert!(start_video_mp4(&mut fixture.app, request).is_err());
    assert_eq!(counts(&fixture.app), before);
    fixture
        .app
        .connection
        .execute_batch("DROP TRIGGER reject_frame_audit")
        .unwrap();
    let job = fixture.submit("original");
    let mut input: PreparedMediaVideoMp4 = serde_json::from_value(job.input).unwrap();
    *input.processing_policy.stalled_after_ms.as_mut().unwrap() -= 1;
    let request = fixture.request("forged");
    let before = counts(&fixture.app);
    assert_eq!(
        fixture
            .app
            .create_prepared_job(
                JobRequest {
                    idempotency_key: "forged".into(),
                    kind: MEDIA_VIDEO_MP4_KIND.into(),
                    input: serde_json::to_value(request).unwrap()
                },
                serde_json::to_value(input).unwrap()
            )
            .unwrap_err()
            .code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(counts(&fixture.app), before);
}
