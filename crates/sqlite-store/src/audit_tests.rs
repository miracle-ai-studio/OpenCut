use super::*;
use opencut_editor_application::audit::{AuditQuery, AuditTarget, CallSource, ExecutionContext};

#[test]
fn journal_conversion_releases_its_read_lock_before_retry() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("workspace.sqlite");
    drop(EditorApplication::open(&database).unwrap());
    let reader = Connection::open(&database).unwrap();
    reader
        .execute_batch("PRAGMA journal_mode=DELETE; BEGIN; SELECT * FROM metadata;")
        .unwrap();
    let converter = Connection::open(&database).unwrap();
    converter.busy_timeout(Duration::ZERO).unwrap();
    // The read transaction demonstrably prevents the exclusive conversion.
    assert!(
        matches!(converter.query_row::<String,_,_>("PRAGMA journal_mode=WAL",[],|r|r.get(0)),Err(rusqlite::Error::SqliteFailure(e,_)) if e.code==ErrorCode::DatabaseBusy)
    );
    converter.busy_timeout(Duration::from_secs(5)).unwrap();
    let pending = std::thread::spawn(move || ensure_wal_for_file_database(&converter));
    std::thread::sleep(Duration::from_millis(30));
    reader.execute_batch("COMMIT").unwrap();
    pending.join().unwrap().unwrap();
    let reopened = EditorApplication::open(database).unwrap();
    let mode: String = reopened
        .connection
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
}

#[test]
fn journal_conversion_respects_and_restores_the_busy_deadline() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("workspace.sqlite");
    drop(EditorApplication::open(&database).unwrap());
    let reader = Connection::open(&database).unwrap();
    reader
        .execute_batch("PRAGMA journal_mode=DELETE; BEGIN; SELECT * FROM metadata;")
        .unwrap();
    let converter = Connection::open(&database).unwrap();
    converter.busy_timeout(Duration::from_millis(50)).unwrap();
    let start = std::time::Instant::now();
    assert!(ensure_wal_for_file_database(&converter).is_err());
    assert!(start.elapsed() < Duration::from_secs(2));
    let timeout: u64 = converter
        .query_row("PRAGMA busy_timeout", [], |r| r.get(0))
        .unwrap();
    assert_eq!(timeout, 50);
    reader.execute_batch("COMMIT").unwrap();
    ensure_wal_for_file_database(&converter).unwrap();
}

fn context(app: &mut EditorApplication, source: CallSource) -> String {
    let context = ExecutionContext::local(source);
    let id = context.request_id().to_owned();
    app.set_execution_context(context);
    id
}
fn all(app: &EditorApplication) -> Vec<opencut_api_contracts::audit::AuditRecord> {
    app.audit_query(AuditQuery {
        target: AuditTarget::Workspace,
        after_sequence: "0".into(),
        limit: 500,
    })
    .unwrap()
    .records
}
fn fault(app: &EditorApplication, action: &str) {
    app.connection.execute_batch(&format!("CREATE TRIGGER fail_audit BEFORE INSERT ON audit_records WHEN NEW.action='{action}' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;")).unwrap();
}
fn clear(app: &EditorApplication) {
    app.connection
        .execute_batch("DROP TRIGGER fail_audit")
        .unwrap();
}
fn input() -> CreateProject {
    CreateProject {
        id: "p".into(),
        name: "P".into(),
        sequence_id: "main".into(),
        width: 64,
        height: 48,
        fps: opencut_contracts::FrameRate {
            numerator: 30,
            denominator: 1,
        },
    }
}
fn job() -> JobRequest {
    JobRequest {
        idempotency_key: "job".into(),
        kind: "media.probe".into(),
        input: json!({"input":"/private/source.wav"}),
    }
}

#[test]
fn project_audit_failure_rolls_back_and_replay_retains_original_actor() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    fault(&app, "project.created");
    assert_eq!(
        app.create_project(input(), "create").unwrap_err().code,
        "STORAGE_ERROR"
    );
    assert!(app.list_projects().unwrap().is_empty());
    assert!(all(&app).is_empty());
    clear(&app);
    let create_id = context(&mut app, CallSource::Cli);
    let initial = app.create_project(input(), "create").unwrap();
    assert_eq!(all(&app)[0].request_id, create_id);
    let request = TransactionRequest {
        expected_revision: initial.revision.clone(),
        idempotency_key: "edit".into(),
        label: "Rename".into(),
        operations: vec![opencut_contracts::Operation::UpdateSequenceSettings {
            sequence_id: "main".into(),
            name: Some("New".into()),
            width: None,
            height: None,
            fps: None,
        }],
    };
    fault(&app, "project.applied");
    assert_eq!(app.apply("p", &request).unwrap_err().code, "STORAGE_ERROR");
    assert_eq!(app.snapshot("p").unwrap(), initial);
    assert_eq!(all(&app).len(), 1);
    clear(&app);
    let edit_id = context(&mut app, CallSource::Http);
    let edited = app.apply("p", &request).unwrap();
    context(&mut app, CallSource::McpStdio);
    assert_eq!(app.apply("p", &request).unwrap(), edited);
    let records = all(&app);
    assert_eq!(records.len(), 2);
    assert_eq!(records[1].request_id, edit_id);
    assert_eq!(records[1].actor.source, CallSource::Http);
    assert_eq!(
        records[1].transaction_id.as_deref(),
        Some(edited.transaction_id.as_str())
    );
    app.restore_revision(
        "p",
        &RestoreRevisionRequest {
            expected_revision: edited.revision,
            target_revision: initial.revision,
            idempotency_key: "restore".into(),
            label: "Restore".into(),
        },
    )
    .unwrap();
    assert_eq!(all(&app)[2].action, "project.restored");
}

#[test]
fn job_accept_and_cancel_are_atomic_with_provenance() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    fault(&app, "job.accepted");
    assert_eq!(app.create_job(job()).unwrap_err().code, "STORAGE_ERROR");
    assert!(app.list_jobs().unwrap().is_empty());
    clear(&app);
    let origin = context(&mut app, CallSource::McpHttp);
    let accepted = app.create_job(job()).unwrap();
    let cancel_id = context(&mut app, CallSource::Cli);
    fault(&app, "job.cancelled");
    assert!(app.cancel_job(&accepted.job_id).is_err());
    assert_eq!(
        app.get_job(&accepted.job_id).unwrap().state,
        JobState::Queued
    );
    assert_eq!(all(&app).len(), 1);
    clear(&app);
    let cancelled = app.cancel_job(&accepted.job_id).unwrap();
    assert_eq!(app.cancel_job(&accepted.job_id).unwrap(), cancelled);
    let records = all(&app);
    assert_eq!(records.len(), 2);
    assert_eq!(records[1].request_id, cancel_id);
    assert_eq!(
        records[1].origin_request_id.as_deref(),
        Some(origin.as_str())
    );
    assert!(
        !serde_json::to_string(&records)
            .unwrap()
            .contains("/private/")
    );
}

#[test]
fn recovered_attempts_link_origin_and_failed_completion_cannot_append_audit() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let origin = context(&mut app, CallSource::Http);
    let accepted = app.create_job(job()).unwrap();
    let first = app.claim_job("worker-a", 100, 10).unwrap().unwrap();
    app.recover_expired(111).unwrap();
    let second = app.claim_job("worker-b", 112, 20).unwrap().unwrap();
    assert_eq!(second.attempt, 2);
    let before = all(&app).len();
    assert!(
        app.complete_job(&accepted.job_id, "worker-a", first.attempt, 113, json!({}))
            .is_err()
    );
    fault(&app, "job.succeeded");
    assert!(
        app.complete_job(&accepted.job_id, "worker-b", second.attempt, 113, json!({}))
            .is_err()
    );
    assert_eq!(
        app.get_job(&accepted.job_id).unwrap().state,
        JobState::Running
    );
    assert_eq!(all(&app).len(), before);
    clear(&app);
    app.complete_job(&accepted.job_id, "worker-b", second.attempt, 114, json!({}))
        .unwrap();
    let records = all(&app);
    assert!(
        records
            .iter()
            .all(|r| r.origin_request_id.as_deref() == Some(origin.as_str()))
    );
    let completed = records.last().unwrap();
    assert_eq!(completed.actor.worker_id.as_deref(), Some("worker-b"));
    assert_eq!(completed.actor.source, CallSource::Worker);
    assert_eq!(completed.attempt, Some(2));
    assert_ne!(completed.request_id, origin);
}

#[test]
fn legacy_job_keeps_unknown_origin_and_large_sequences_are_exact() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    app.create_job(job()).unwrap();
    app.connection
        .execute("DELETE FROM audit_records", [])
        .unwrap();
    app.claim_job("worker", 1, 20).unwrap().unwrap();
    app.connection
        .execute("UPDATE audit_records SET sequence=9007199254741017", [])
        .unwrap();
    let page = app
        .audit_query(AuditQuery {
            target: AuditTarget::Workspace,
            after_sequence: "9007199254741016".into(),
            limit: 1,
        })
        .unwrap();
    assert_eq!(page.last_sequence, "9007199254741017");
    assert!(page.records[0].origin_request_id.is_none());
    for after in ["01", "-1", "1e3", "9223372036854775808"] {
        assert_eq!(
            app.audit_query(AuditQuery {
                target: AuditTarget::Workspace,
                after_sequence: after.into(),
                limit: 1
            })
            .unwrap_err()
            .code,
            "INVALID_ARGUMENT"
        );
    }
}
