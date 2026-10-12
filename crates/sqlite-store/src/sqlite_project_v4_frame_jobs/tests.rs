//! Queue/authorization tests. Real GPU/PNG acceptance belongs to the native suite.
use super::*;
use opencut_contracts::{CreateProject, FrameRate, Time, project_v4_ops::*};
use opencut_editor_application::access::{Caller, PrincipalPolicy, ProjectGrant};

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
        let snapshot = app
            .create_v4_project(&CreateV4Project {
                project: CreateProject {
                    id: "frame-project".into(),
                    name: "Fixed frames".into(),
                    sequence_id: "scene".into(),
                    width: 64,
                    height: 48,
                    fps: FrameRate {
                        numerator: 24,
                        denominator: 1,
                    },
                },
                idempotency_key: "create".into(),
            })
            .unwrap();
        Self {
            app,
            directory,
            revision: snapshot.revision,
        }
    }
    fn request(&self, key: &str) -> SubmitV4FramePng {
        SubmitV4FramePng {
            idempotency_key: key.into(),
            project_id: "frame-project".into(),
            revision: self.revision.clone(),
            sequence_id: "scene".into(),
            time: Time {
                ticks: "1".into(),
                timebase: 3,
            },
        }
    }
    fn submit(&mut self, key: &str) -> JobRecord {
        let request = self.request(key);
        start_frame_png(&mut self.app, request).unwrap()
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
                        project_id: "frame-project".into(),
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
    let captured: PreparedV4FramePng = serde_json::from_value(job.input.clone()).unwrap();
    assert_eq!(captured.time, fixture.request("fixed").time);
    assert_eq!(captured.snapshot.revision, fixture.revision);
    assert!(captured.asset_lock.assets.is_empty());
    fixture
        .app
        .apply_v4_project(&ApplyV4Project {
            project_id: "frame-project".into(),
            expected_revision: fixture.revision.clone(),
            idempotency_key: "rename".into(),
            label: "Rename after fixed frame acceptance".into(),
            operations: vec![ProjectOperationV4::Rename {
                name: "Later head".into(),
            }],
        })
        .unwrap();
    assert_eq!(fixture.submit("fixed"), job);
    let mut changed = fixture.request("fixed");
    changed.time.ticks = "2".into();
    assert!(start_frame_png(&mut fixture.app, changed).is_err());
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
        let input: PreparedV4FramePng = serde_json::from_value(running.input.clone()).unwrap();
        fixture
            .app
            .revalidate_v4_frame_job(
                &job.job_id,
                "frame-worker",
                running.attempt,
                &input.authority,
            )
            .unwrap();
        assert_eq!(
            fixture
                .app
                .revalidate_v4_frame_job(
                    &job.job_id,
                    "frame-worker",
                    running.attempt,
                    &TrustedJobAuthorityV1::LocalOwner
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
                .revalidate_v4_frame_job(
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
        let input: PreparedV4FramePng = serde_json::from_value(running.input.clone()).unwrap();
        assert_eq!(
            fixture
                .app
                .revalidate_v4_frame_job(
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
    assert!(start_frame_png(&mut fixture.app, request).is_err());
    assert_eq!(counts(&fixture.app), before);
    fixture
        .app
        .connection
        .execute_batch("DROP TRIGGER reject_frame_audit")
        .unwrap();
    let job = fixture.submit("original");
    let mut input: PreparedV4FramePng = serde_json::from_value(job.input).unwrap();
    input.processing_policy.deadline_ms -= 1;
    let request = fixture.request("forged");
    let before = counts(&fixture.app);
    assert_eq!(
        fixture
            .app
            .create_prepared_job(
                JobRequest {
                    idempotency_key: "forged".into(),
                    kind: V4_FRAME_PNG_KIND.into(),
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
