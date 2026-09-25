use super::*;
use opencut_asset_store::uploads::UploadChunks;
use opencut_editor_application::{access::*, uploads::*};
use sha2::{Digest, Sha256};

fn project(app: &mut EditorApplication, id: &str) -> ProjectSnapshot {
    app.create_project(
        CreateProject {
            id: id.into(),
            name: id.into(),
            sequence_id: "main".into(),
            width: 64,
            height: 48,
            fps: opencut_contracts::FrameRate {
                numerator: 10,
                denominator: 1,
            },
        },
        &format!("create-{id}"),
    )
    .unwrap()
}
fn input(key: &str) -> CreateUpload {
    CreateUpload {
        project_id: "a".into(),
        idempotency_key: key.into(),
        name: "../display-only.mov".into(),
        size_bytes: "6".into(),
        content_hash: format!("{:x}", Sha256::digest(b"abcdef")),
    }
}
fn chunk(id: &str, offset: &str, bytes: &[u8]) -> ChunkDescriptor {
    ChunkDescriptor {
        upload_id: id.into(),
        offset: offset.into(),
        size_bytes: bytes.len().to_string(),
        content_hash: format!("{:x}", Sha256::digest(bytes)),
    }
}
fn counts(app: &EditorApplication) -> Vec<i64> {
    [
        "upload_sessions",
        "upload_chunks",
        "project_media",
        "revisions",
        "jobs",
        "outbox",
        "audit_records",
        "job_events",
        "job_projects",
    ]
    .iter()
    .map(|table| {
        app.connection
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    })
    .collect()
}
fn policy(actions: Vec<ProjectAction>, revision: &str) -> PrincipalPolicy {
    PrincipalPolicy {
        principal_id: "uploader".into(),
        expected_revision: revision.into(),
        enabled: true,
        grants: vec![ProjectGrant {
            project_id: "a".into(),
            actions,
        }],
    }
}
fn all() -> Vec<ProjectAction> {
    vec![
        ProjectAction::Read,
        ProjectAction::Edit,
        ProjectAction::RunJobs,
    ]
}
fn code<T>(result: Result<T, EditorError>, expected: &str) {
    assert!(
        matches!(result, Err(ref e) if e.code == expected),
        "expected {expected}"
    );
}

#[test]
fn upload_replay_is_exact_and_never_moves_the_project_head() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut app = EditorApplication::open(&path).unwrap();
    let snapshot = project(&mut app, "a");
    let bytes = UploadChunks::new(dir.path().join("uploads"));
    let view = app.create_upload(&input("create")).unwrap();
    let first = chunk(&view.upload_id, "0", b"abc");
    assert_eq!(
        bytes.put(&mut app, &first, b"abc").unwrap().next_offset,
        "3"
    );
    let before = counts(&app);
    assert_eq!(
        app.create_upload(&input("create")).unwrap().next_offset,
        "3"
    );
    assert_eq!(
        bytes.put(&mut app, &first, b"abc").unwrap().next_offset,
        "3"
    );
    assert_eq!(counts(&app), before);
    code(
        bytes.put(&mut app, &chunk(&view.upload_id, "0", b"xyz"), b"xyz"),
        "UPLOAD_CHUNK_CONFLICT",
    );
    code(
        bytes.put(&mut app, &chunk(&view.upload_id, "1", b"bc"), b"bc"),
        "UPLOAD_CHUNK_CONFLICT",
    );
    code(
        bytes.put(&mut app, &chunk(&view.upload_id, "2", b"cd"), b"cd"),
        "UPLOAD_OFFSET_CONFLICT",
    );
    code(
        bytes.put(&mut app, &chunk(&view.upload_id, "4", b"ef"), b"ef"),
        "UPLOAD_OFFSET_CONFLICT",
    );
    code(
        bytes.put(&mut app, &chunk(&view.upload_id, "3", b"defg"), b"defg"),
        "UPLOAD_OFFSET_CONFLICT",
    );
    let mut changed = input("create");
    changed.name = "different".into();
    code(app.create_upload(&changed), "IDEMPOTENCY_KEY_REUSED");
    assert_eq!(counts(&app), before);
    drop(app);
    let mut reopened = EditorApplication::open(&path).unwrap();
    assert_eq!(
        reopened
            .inspect_upload(&view.upload_id)
            .unwrap()
            .next_offset,
        "3"
    );
    assert_eq!(
        bytes
            .put(&mut reopened, &chunk(&view.upload_id, "3", b"def"), b"def")
            .unwrap()
            .next_offset,
        "6"
    );
    assert_eq!(reopened.snapshot("a").unwrap(), snapshot);
    assert_eq!(reopened.list_jobs().unwrap().len(), 0);
    assert_eq!(
        std::fs::read(bytes.root().join(&view.upload_id).join("0.chunk")).unwrap(),
        b"abc"
    );
}

#[test]
fn disk_failure_and_bad_bytes_never_acknowledge_a_chunk() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = EditorApplication::open(":memory:").unwrap();
    project(&mut app, "a");
    let view = app.create_upload(&input("create")).unwrap();
    let desc = chunk(&view.upload_id, "0", b"abc");
    let root = dir.path().join("private-host-path");
    std::fs::write(&root, b"not a directory").unwrap();
    let bytes = UploadChunks::new(&root);
    let before = counts(&app);
    code(
        bytes.put(&mut app, &desc, b"wrong"),
        "UPLOAD_CONTENT_MISMATCH",
    );
    let err = bytes.put(&mut app, &desc, b"abc").unwrap_err();
    assert_eq!(err.code, "UPLOAD_STORAGE_ERROR");
    assert!(
        !serde_json::to_string(&err)
            .unwrap()
            .contains("private-host-path")
    );
    assert_eq!(counts(&app), before);
    assert_eq!(
        app.inspect_upload(&view.upload_id).unwrap().next_offset,
        "0"
    );
}

#[test]
fn audit_failure_rolls_back_offset_but_durable_bytes_can_be_retried() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = EditorApplication::open(dir.path().join("db")).unwrap();
    project(&mut app, "a");
    let bytes = UploadChunks::new(dir.path().join("uploads"));
    let before_create = counts(&app);
    app.connection.execute_batch("CREATE TRIGGER deny_upload BEFORE INSERT ON audit_records BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;").unwrap();
    assert!(app.create_upload(&input("create")).is_err());
    assert_eq!(counts(&app), before_create);
    app.connection
        .execute_batch("DROP TRIGGER deny_upload;")
        .unwrap();
    let view = app.create_upload(&input("create")).unwrap();
    let desc = chunk(&view.upload_id, "0", b"abc");
    let before = counts(&app);
    app.connection.execute_batch("CREATE TRIGGER deny_upload BEFORE INSERT ON audit_records BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;").unwrap();
    assert!(bytes.put(&mut app, &desc, b"abc").is_err());
    assert_eq!(counts(&app), before);
    assert_eq!(
        app.inspect_upload(&view.upload_id).unwrap().next_offset,
        "0"
    );
    let target = bytes.root().join(&view.upload_id).join("0.chunk");
    assert_eq!(std::fs::read(&target).unwrap(), b"abc");
    app.connection
        .execute_batch("DROP TRIGGER deny_upload;")
        .unwrap();
    assert_eq!(bytes.put(&mut app, &desc, b"abc").unwrap().next_offset, "3");
    // A confirmed corrupt object must not be silently replaced even on exact replay.
    std::fs::write(&target, b"bad").unwrap();
    code(bytes.put(&mut app, &desc, b"abc"), "UPLOAD_INTEGRITY_ERROR");
    assert_eq!(std::fs::read(&target).unwrap(), b"bad");
    std::fs::remove_file(&target).unwrap();
    code(bytes.put(&mut app, &desc, b"abc"), "UPLOAD_INTEGRITY_ERROR");
    assert_eq!(
        app.inspect_upload(&view.upload_id).unwrap().next_offset,
        "3"
    );
}

#[test]
fn upload_authorization_precedes_receipt_replay_and_uses_record_project() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = EditorApplication::open(":memory:").unwrap();
    project(&mut app, "a");
    project(&mut app, "b");
    let own = app.create_upload(&input("create")).unwrap();
    let mut foreign = input("create");
    foreign.project_id = "b".into();
    let foreign = app.create_upload(&foreign).unwrap();
    app.set_principal_policy(policy(all(), "0")).unwrap();
    let credential = app.issue_credential("uploader").unwrap();
    let caller = app.authenticate(&credential.secret).unwrap();
    app.set_caller(caller.clone());
    let bytes = UploadChunks::new(dir.path().join("uploads"));
    let before = counts(&app);
    code(app.inspect_upload(&foreign.upload_id), "NOT_AUTHORIZED");
    code(
        app.inspect_upload("upload_00000000000000000000000000000000"),
        "NOT_AUTHORIZED",
    );
    code(
        bytes.put(&mut app, &chunk(&foreign.upload_id, "0", b"abc"), b"abc"),
        "NOT_AUTHORIZED",
    );
    code(app.abort_upload(&foreign.upload_id), "NOT_AUTHORIZED");
    assert!(!bytes.root().exists());
    assert_eq!(counts(&app), before);
    let desc = chunk(&own.upload_id, "0", b"abc");
    bytes.put(&mut app, &desc, b"abc").unwrap();
    for (index, actions) in [
        vec![ProjectAction::Read],
        vec![ProjectAction::Read, ProjectAction::Edit],
        vec![ProjectAction::Read, ProjectAction::RunJobs],
    ]
    .into_iter()
    .enumerate()
    {
        app.set_caller(Caller::LocalOwner);
        app.set_principal_policy(policy(actions, &(index + 1).to_string()))
            .unwrap();
        app.set_caller(caller.clone());
        assert_eq!(app.inspect_upload(&own.upload_id).unwrap().next_offset, "3");
        let before = counts(&app);
        code(app.create_upload(&input("create")), "NOT_AUTHORIZED");
        code(bytes.put(&mut app, &desc, b"abc"), "NOT_AUTHORIZED");
        code(app.abort_upload(&own.upload_id), "NOT_AUTHORIZED");
        assert_eq!(counts(&app), before);
    }
    app.set_caller(Caller::LocalOwner);
    app.set_principal_policy(policy(all(), "4")).unwrap();
    app.revoke_credential(&credential.credential_id).unwrap();
    app.set_caller(caller);
    code(app.inspect_upload(&own.upload_id), "NOT_AUTHORIZED");
    code(app.create_upload(&input("create")), "NOT_AUTHORIZED");
    code(bytes.put(&mut app, &desc, b"abc"), "NOT_AUTHORIZED");
}

#[test]
fn abort_is_atomic_and_idempotent_and_releases_receiving_quota() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    project(&mut app, "a");
    let views = (0..8)
        .map(|n| app.create_upload(&input(&n.to_string())).unwrap())
        .collect::<Vec<_>>();
    code(app.create_upload(&input("overflow")), "RESOURCE_TOO_LARGE");
    let before = counts(&app);
    app.connection.execute_batch("CREATE TRIGGER deny_abort BEFORE INSERT ON outbox BEGIN SELECT RAISE(ABORT,'event unavailable'); END;").unwrap();
    assert!(app.abort_upload(&views[0].upload_id).is_err());
    assert_eq!(counts(&app), before);
    assert_eq!(
        app.inspect_upload(&views[0].upload_id).unwrap().state,
        UploadState::Receiving
    );
    app.connection
        .execute_batch("DROP TRIGGER deny_abort;")
        .unwrap();
    assert_eq!(
        app.abort_upload(&views[0].upload_id).unwrap().state,
        UploadState::Aborted
    );
    let before = counts(&app);
    app.abort_upload(&views[0].upload_id).unwrap();
    assert_eq!(
        app.create_upload(&input("0")).unwrap().state,
        UploadState::Aborted
    );
    assert_eq!(counts(&app), before);
    code(
        app.authorize_chunk(&chunk(&views[0].upload_id, "0", b"abc")),
        "UPLOAD_STATE_CONFLICT",
    );
    assert!(app.create_upload(&input("overflow")).is_ok());
}

#[test]
fn upload_migration_rolls_back_all_tables_on_collision() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let original = {
        let mut app = EditorApplication::open(&path).unwrap();
        project(&mut app, "a")
    };
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch(sqlite_artifact_publication::DROP_FOR_TEST)
        .unwrap();
    conn.execute_batch("DROP TABLE project_preset_grants; DROP TABLE project_edit_control; DROP TABLE project_merge_parents; DROP TABLE project_branches; DROP TABLE artifact_gc_plans; DROP TABLE artifact_retention; DROP TABLE cas_gc_events; DROP TABLE cas_gc_plans; DROP TABLE denial_records; DROP TABLE project_plugin_grants; DROP TABLE upload_retention; DROP TABLE project_media; DROP TABLE upload_chunks; DROP TABLE upload_sessions; UPDATE metadata SET value='7' WHERE key='schema_version'; CREATE TABLE upload_chunks(collision TEXT);").unwrap();
    assert!(EditorApplication::open(&path).is_err());
    assert_eq!(
        conn.query_row::<String, _, _>(
            "SELECT value FROM metadata WHERE key='schema_version'",
            [],
            |r| r.get(0)
        )
        .unwrap(),
        "7"
    );
    assert_eq!(
        conn.query_row::<i64, _, _>(
            "SELECT count(*) FROM sqlite_master WHERE name IN ('upload_sessions','project_media')",
            [],
            |r| r.get(0)
        )
        .unwrap(),
        0
    );
    conn.execute_batch("DROP TABLE upload_chunks;").unwrap();
    let app = EditorApplication::open(&path).unwrap();
    assert_eq!(app.snapshot("a").unwrap(), original);
    assert_eq!(
        app.connection
            .query_row::<String, _, _>(
                "SELECT value FROM metadata WHERE key='schema_version'",
                [],
                |r| r.get(0)
            )
            .unwrap(),
        DATABASE_SCHEMA_VERSION.to_string()
    );
}

#[test]
fn chunk_count_quota_bounds_metadata_and_still_allows_exact_replay() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    project(&mut app, "a");
    let mut request = input("create");
    request.size_bytes = (MAX_UPLOAD_CHUNKS + 1).to_string();
    let view = app.create_upload(&request).unwrap();
    let desc = chunk(&view.upload_id, "0", b"a");
    // Seed a complete confirmed prefix, avoiding 4096 redundant durable disk writes.
    let tx = app.connection.transaction().unwrap();
    for offset in 0..MAX_UPLOAD_CHUNKS {
        tx.execute("INSERT INTO upload_chunks(upload_id,offset_bytes,size_bytes,content_hash,request_id) VALUES (?1,?2,1,?3,'seed')", params![view.upload_id,offset,desc.content_hash]).unwrap();
    }
    tx.execute(
        "UPDATE upload_sessions SET next_offset=?1 WHERE upload_id=?2",
        params![MAX_UPLOAD_CHUNKS, view.upload_id],
    )
    .unwrap();
    tx.commit().unwrap();
    assert!(app.authorize_chunk(&desc).is_ok());
    let next = chunk(&view.upload_id, &MAX_UPLOAD_CHUNKS.to_string(), b"a");
    let before = counts(&app);
    code(app.authorize_chunk(&next), "RESOURCE_TOO_LARGE");
    code(app.acknowledge_chunk(&next), "RESOURCE_TOO_LARGE");
    assert_eq!(counts(&app), before);
}

#[test]
fn untrusted_upload_parameters_are_rejected_before_native_storage() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = EditorApplication::open(":memory:").unwrap();
    project(&mut app, "a");
    let before = counts(&app);
    for size in [
        "0",
        "01",
        "-1",
        "+1",
        "1.0",
        "1073741825",
        "18446744073709551616",
    ] {
        let mut request = input(size);
        request.size_bytes = size.into();
        code(app.create_upload(&request), "INVALID_ARGUMENT");
    }
    for hash in ["abc", &"A".repeat(64), &"g".repeat(64)] {
        let mut request = input("bad-hash");
        request.content_hash = hash.into();
        code(app.create_upload(&request), "INVALID_ARGUMENT");
    }
    assert_eq!(counts(&app), before);
    let view = app.create_upload(&input("good")).unwrap();
    let bytes = UploadChunks::new(dir.path().join("uploads"));
    for id in ["../../escape", "upload_../escape", "upload_0000"] {
        code(
            bytes.put(&mut app, &chunk(id, "0", b"abc"), b"abc"),
            "INVALID_ARGUMENT",
        );
    }
    for offset in ["00", "-1", "18446744073709551615"] {
        code(
            bytes.put(&mut app, &chunk(&view.upload_id, offset, b"abc"), b"abc"),
            "INVALID_ARGUMENT",
        );
    }
    assert!(!bytes.root().exists());
}

fn sealed(app: &mut EditorApplication) -> (UploadView, JobRecord, MediaImportJobResult) {
    project(app, "a");
    let upload = app.create_upload(&input("create")).unwrap();
    app.acknowledge_chunk(&chunk(&upload.upload_id, "0", b"abcdef"))
        .unwrap();
    let upload = app.seal_upload(&upload.upload_id).unwrap();
    let job = app.get_job(upload.job_id.as_deref().unwrap()).unwrap();
    let input: PreparedMediaImportInput = serde_json::from_value(job.input.clone()).unwrap();
    let result = MediaImportJobResult { media_id: input.media_id.clone(), project_id: input.project_id, upload_id: input.upload_id, job_id: job.job_id.clone(),
        asset: opencut_contracts::Asset { id: input.media_id.clone(), name: input.name, kind: opencut_contracts::AssetKind::Image,
            uri: format!("opencut-media://{}",input.media_id), duration: None,
            version: Some(opencut_contracts::AssetVersion { id: format!("av_{}",input.content_hash), content_hash: input.content_hash.clone(), size: input.size_bytes }),
            metadata: serde_json::from_value(json!({"content_hash":input.content_hash,"size":input.size_bytes,"mime_type":"image/png","has_audio":false})).unwrap() } };
    (upload, job, result)
}
#[test]
fn sealing_and_accepting_the_scoped_import_are_one_idempotent_transaction() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let snapshot = project(&mut app, "a");
    let upload = app.create_upload(&input("create")).unwrap();
    code(app.seal_upload(&upload.upload_id), "UPLOAD_INCOMPLETE");
    app.acknowledge_chunk(&chunk(&upload.upload_id, "0", b"abcdef"))
        .unwrap();
    let before = counts(&app);
    app.connection.execute_batch("CREATE TRIGGER deny_import BEFORE INSERT ON job_events BEGIN SELECT RAISE(ABORT,'queue event unavailable'); END;").unwrap();
    assert!(app.seal_upload(&upload.upload_id).is_err());
    assert_eq!(counts(&app), before);
    assert_eq!(
        app.inspect_upload(&upload.upload_id).unwrap().state,
        UploadState::Receiving
    );
    app.connection
        .execute_batch("DROP TRIGGER deny_import;")
        .unwrap();
    app.set_principal_policy(policy(all(), "0")).unwrap();
    let issued = app.issue_credential("uploader").unwrap();
    let caller = app.authenticate(&issued.secret).unwrap();
    app.set_caller(caller.clone());
    let accepted = app.seal_upload(&upload.upload_id).unwrap();
    let before = counts(&app);
    assert_eq!(app.seal_upload(&upload.upload_id).unwrap(), accepted);
    assert_eq!(counts(&app), before);
    let view = app.job_view(accepted.job_id.as_deref().unwrap()).unwrap();
    assert_eq!(view.resource_target.unwrap().project_id, "a");
    assert_eq!(app.snapshot("a").unwrap(), snapshot);
    code(app.abort_upload(&upload.upload_id), "UPLOAD_STATE_CONFLICT");
    app.set_caller(Caller::LocalOwner);
    app.revoke_credential(&issued.credential_id).unwrap();
    app.set_caller(caller);
    code(app.seal_upload(&upload.upload_id), "NOT_AUTHORIZED");
}
#[test]
fn import_success_resource_events_and_audit_commit_or_roll_back_together() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let (_, job, result) = sealed(&mut app);
    let head = app.snapshot("a").unwrap();
    let running = app.claim_job("worker", 100, 100).unwrap().unwrap();
    let before = counts(&app);
    let mut forged = result.clone();
    forged.asset.uri = "/private/host/file".into();
    code(
        app.complete_job(&job.job_id, "worker", running.attempt, 101, json!(forged)),
        "INVALID_JOB_RESULT",
    );
    assert_eq!(counts(&app), before);
    let mut forged = result.clone();
    forged
        .asset
        .metadata
        .insert("private_path".into(), json!("/secret"));
    code(
        app.complete_job(&job.job_id, "worker", running.attempt, 101, json!(forged)),
        "INVALID_JOB_RESULT",
    );
    app.connection.execute_batch("CREATE TRIGGER deny_import BEFORE INSERT ON audit_records WHEN NEW.action='job.succeeded' BEGIN SELECT RAISE(ABORT,'completion audit unavailable'); END;").unwrap();
    assert!(
        app.complete_job(&job.job_id, "worker", running.attempt, 102, json!(result))
            .is_err()
    );
    assert_eq!(counts(&app), before);
    assert_eq!(app.get_job(&job.job_id).unwrap().state, JobState::Running);
    app.connection
        .execute_batch("DROP TRIGGER deny_import;")
        .unwrap();
    app.complete_job(&job.job_id, "worker", running.attempt, 103, json!(result))
        .unwrap();
    assert_eq!(
        app.project_media(&ProjectMediaInput {
            project_id: "a".into(),
            media_id: result.media_id.clone()
        })
        .unwrap(),
        result
    );
    assert_eq!(app.snapshot("a").unwrap(), head);
    let before_cancel = counts(&app);
    assert_eq!(
        app.cancel_job(&job.job_id).unwrap().state,
        JobState::Succeeded
    );
    assert_eq!(counts(&app), before_cancel);
    let view = app.job_view(&job.job_id).unwrap();
    assert!(view.result.is_none() && view.result_unavailable.is_none());
    assert_eq!(view.extended_result.unwrap().data, json!(result));
    let links: (String,String) = app.connection.query_row("SELECT project_id,origin_request_id FROM audit_records WHERE job_id=?1 AND action='job.succeeded'", [&job.job_id], |r|Ok((r.get(0)?,r.get(1)?))).unwrap();
    assert_eq!(links.0, "a");
    assert!(!links.1.is_empty());
}
#[test]
fn cancelled_expired_and_superseded_import_attempts_cannot_register_media() {
    for mode in ["cancel", "expired", "superseded"] {
        let mut app = EditorApplication::open(":memory:").unwrap();
        let (_, job, result) = sealed(&mut app);
        let running = app.claim_job("worker", 100, 10).unwrap().unwrap();
        if mode == "cancel" {
            app.cancel_job(&job.job_id).unwrap();
        }
        if mode == "superseded" {
            app.recover_expired(111).unwrap();
            app.claim_job("next", 112, 100).unwrap();
        }
        let before = counts(&app);
        let completion = app.complete_job(
            &job.job_id,
            "worker",
            running.attempt,
            if mode == "cancel" { 105 } else { 113 },
            json!(result),
        );
        if mode == "cancel" {
            let cancelled = completion.unwrap();
            assert_eq!(cancelled.state, JobState::Cancelled);
            assert!(cancelled.result.is_none());
            assert_eq!(&counts(&app)[..6], &before[..6]);
        } else {
            assert!(completion.is_err());
            assert_eq!(counts(&app), before);
        }
        assert_eq!(
            app.connection
                .query_row::<i64, _, _>("SELECT count(*) FROM project_media", [], |r| r.get(0))
                .unwrap(),
            0
        );
    }
}

#[test]
fn abort_cleanup_failure_is_committed_and_retry_removes_only_private_chunks() {
    struct FailedCleanup;
    impl UploadCleanup for FailedCleanup {
        fn discard_chunks(
            &self,
            _: &str,
            _: &dyn Fn() -> Result<(), EditorError>,
        ) -> Result<(), EditorError> {
            Err(EditorError::new("IO_ERROR", "private path must not escape"))
        }
    }
    let root = tempfile::tempdir().unwrap();
    let mut app = EditorApplication::open(root.path().join("workspace.sqlite")).unwrap();
    project(&mut app, "a");
    let view = app.create_upload(&input("cleanup")).unwrap();
    let native = UploadChunks::new(root.path().join("uploads"));
    native
        .put(&mut app, &chunk(&view.upload_id, "0", b"abc"), b"abc")
        .unwrap();
    let request = UploadInput {
        upload_id: view.upload_id.clone(),
    };
    code(
        abort_with_cleanup(&mut app, &FailedCleanup, request.clone()),
        "UPLOAD_CLEANUP_PENDING",
    );
    assert_eq!(
        app.inspect_upload(&view.upload_id).unwrap().state,
        UploadState::Aborted
    );
    assert!(root.path().join("uploads").join(&view.upload_id).exists());
    let before = counts(&app);
    abort_with_cleanup(&mut app, &native, request.clone()).unwrap();
    assert!(!root.path().join("uploads").join(&view.upload_id).exists());
    let mut committed = before;
    committed[5] += 1; // Staging release event and audit commit with reservation release.
    committed[6] += 1;
    assert_eq!(counts(&app), committed);
    assert_eq!(
        app.upload_retention(&view.upload_id).unwrap().state,
        UploadRetentionState::Released
    );
    abort_with_cleanup(&mut app, &native, request).unwrap();
    assert_eq!(counts(&app), committed);
    assert!(root.path().join("workspace.sqlite").exists());
}

#[test]
fn reservations_survive_abort_until_durable_release_and_quota_is_atomic() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    project(&mut app, "a");
    let mut first = None;
    for index in 0..8 {
        let request = CreateUpload {
            size_bytes: MAX_UPLOAD_BYTES.to_string(),
            ..input(&format!("quota-{index}"))
        };
        let view = app.create_upload(&request).unwrap();
        app.abort_upload(&view.upload_id).unwrap(); // State alone cannot free bytes.
        if index == 0 {
            first = Some((view, request));
        }
    }
    assert_eq!(
        app.upload_quota("a").unwrap().project_reserved_bytes,
        PROJECT_UPLOAD_RESERVATION_LIMIT.to_string()
    );
    let before = counts(&app);
    code(
        app.create_upload(&input("over-quota")),
        "UPLOAD_QUOTA_EXCEEDED",
    );
    assert_eq!(counts(&app), before);
    let (first, request) = first.unwrap();
    assert_eq!(
        app.create_upload(&request).unwrap().upload_id,
        first.upload_id
    ); // Replay before capacity checks.
    let root = tempfile::tempdir().unwrap();
    let native = UploadChunks::new(root.path().join("uploads"));
    let released = release_upload(
        &mut app,
        &native,
        UploadInput {
            upload_id: first.upload_id.clone(),
        },
    )
    .unwrap();
    assert_eq!(released.state, UploadRetentionState::Released);
    assert_eq!(released.reserved_bytes, "0");
    let before = counts(&app);
    assert_eq!(
        release_upload(
            &mut app,
            &native,
            UploadInput {
                upload_id: first.upload_id
            }
        )
        .unwrap(),
        released
    );
    assert_eq!(counts(&app), before);
    app.create_upload(&input("over-quota")).unwrap(); // Failed create didn't consume its key.
}

#[test]
fn release_audit_failure_keeps_reservation_after_native_deletion_and_retries() {
    let root = tempfile::tempdir().unwrap();
    let mut app = EditorApplication::open(root.path().join("db")).unwrap();
    project(&mut app, "a");
    let view = app.create_upload(&input("release-fault")).unwrap();
    let native = UploadChunks::new(root.path().join("uploads"));
    native
        .put(&mut app, &chunk(&view.upload_id, "0", b"abc"), b"abc")
        .unwrap();
    code(
        release_upload(
            &mut app,
            &native,
            UploadInput {
                upload_id: view.upload_id.clone(),
            },
        ),
        "UPLOAD_STATE_CONFLICT",
    );
    assert!(root.path().join("uploads").join(&view.upload_id).exists());
    app.abort_upload(&view.upload_id).unwrap();
    app.connection.execute_batch("CREATE TRIGGER deny_release BEFORE INSERT ON audit_records WHEN NEW.action='upload.staging_released' BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    let before = counts(&app);
    code(
        release_upload(
            &mut app,
            &native,
            UploadInput {
                upload_id: view.upload_id.clone(),
            },
        ),
        "UPLOAD_RELEASE_PENDING",
    );
    assert!(!root.path().join("uploads").join(&view.upload_id).exists());
    assert_eq!(counts(&app), before);
    assert_eq!(app.upload_quota("a").unwrap().project_reserved_bytes, "6");
    assert_eq!(
        app.upload_retention(&view.upload_id).unwrap().state,
        UploadRetentionState::Retained
    );
    app.connection
        .execute_batch("DROP TRIGGER deny_release")
        .unwrap();
    release_upload(
        &mut app,
        &native,
        UploadInput {
            upload_id: view.upload_id.clone(),
        },
    )
    .unwrap();
    assert_eq!(app.upload_quota("a").unwrap().project_reserved_bytes, "0");
    assert_eq!(
        app.connection
            .query_row::<i64, _, _>(
                "SELECT count(*) FROM audit_records WHERE action='upload.staging_released'",
                [],
                |r| r.get(0)
            )
            .unwrap(),
        1
    );
}

#[test]
fn active_import_cannot_release_and_terminal_release_preserves_published_media() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let (upload, job, result) = sealed(&mut app);
    let root = tempfile::tempdir().unwrap();
    let native = UploadChunks::new(root.path());
    let request = UploadInput {
        upload_id: upload.upload_id,
    };
    code(
        release_upload(&mut app, &native, request.clone()),
        "UPLOAD_STATE_CONFLICT",
    );
    let running = app.claim_job("worker", 100, 100).unwrap().unwrap();
    code(
        release_upload(&mut app, &native, request.clone()),
        "UPLOAD_STATE_CONFLICT",
    );
    app.complete_job(&job.job_id, "worker", running.attempt, 101, json!(result))
        .unwrap();
    let head = app.snapshot("a").unwrap();
    release_upload(&mut app, &native, request).unwrap();
    assert_eq!(app.snapshot("a").unwrap(), head);
    assert_eq!(
        app.project_media(&ProjectMediaInput {
            project_id: "a".into(),
            media_id: result.media_id.clone()
        })
        .unwrap(),
        result
    );
    assert_eq!(app.get_job(&job.job_id).unwrap().state, JobState::Succeeded);
}

#[test]
fn workspace_reservations_serialize_competing_projects_at_the_last_slot() {
    let root = tempfile::tempdir().unwrap();
    let db = root.path().join("db");
    let mut app = EditorApplication::open(&db).unwrap();
    for i in 0..33 {
        let id = format!("p{i}");
        project(&mut app, &id);
        if i < 31 {
            app.create_upload(&CreateUpload {
                project_id: id,
                size_bytes: MAX_UPLOAD_BYTES.to_string(),
                ..input(&format!("hold-{i}"))
            })
            .unwrap();
        }
    }
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let threads: Vec<_> = (31..33)
        .map(|i| {
            let db = db.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let mut app = EditorApplication::open(db).unwrap();
                barrier.wait();
                app.create_upload(&CreateUpload {
                    project_id: format!("p{i}"),
                    size_bytes: MAX_UPLOAD_BYTES.to_string(),
                    ..input("race")
                })
            })
        })
        .collect();
    let results: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results.into_iter().find_map(Result::err).unwrap().code,
        "UPLOAD_QUOTA_EXCEEDED"
    );
    assert_eq!(
        app.connection
            .query_row::<u64, _, _>(
                "SELECT SUM(reserved_bytes) FROM upload_retention WHERE released=0",
                [],
                |r| r.get(0)
            )
            .unwrap(),
        WORKSPACE_UPLOAD_RESERVATION_LIMIT
    );
}

#[test]
fn release_rechecks_after_lock_and_before_committing_a_freed_reservation() {
    struct RevokingCleanup {
        root: std::path::PathBuf,
        database: std::path::PathBuf,
        credential: String,
        before: bool,
    }
    impl UploadCleanup for RevokingCleanup {
        fn discard_chunks(
            &self,
            id: &str,
            authorize: &dyn Fn() -> Result<(), EditorError>,
        ) -> Result<(), EditorError> {
            let revoke = || {
                EditorApplication::open(&self.database)
                    .unwrap()
                    .revoke_credential(&self.credential)
                    .unwrap();
            };
            if self.before {
                revoke();
            }
            UploadChunks::new(&self.root).discard_chunks(id, authorize)?;
            if !self.before {
                revoke();
            }
            Ok(())
        }
    }
    for before in [true, false] {
        let root = tempfile::tempdir().unwrap();
        let db = root.path().join("db");
        let chunk_root = root.path().join("uploads");
        let mut app = EditorApplication::open(&db).unwrap();
        project(&mut app, "a");
        let upload = app.create_upload(&input("revoke-release")).unwrap();
        UploadChunks::new(&chunk_root)
            .put(&mut app, &chunk(&upload.upload_id, "0", b"abc"), b"abc")
            .unwrap();
        app.abort_upload(&upload.upload_id).unwrap();
        app.set_principal_policy(policy(all(), "0")).unwrap();
        let issued = app.issue_credential("uploader").unwrap();
        app.set_caller(app.authenticate(&issued.secret).unwrap());
        let cleanup = RevokingCleanup {
            root: chunk_root.clone(),
            database: db,
            credential: issued.credential_id,
            before,
        };
        code(
            release_upload(
                &mut app,
                &cleanup,
                UploadInput {
                    upload_id: upload.upload_id.clone(),
                },
            ),
            "NOT_AUTHORIZED",
        );
        assert_eq!(chunk_root.join(&upload.upload_id).exists(), before);
        app.set_caller(Caller::LocalOwner);
        assert_eq!(app.upload_quota("a").unwrap().project_reserved_bytes, "6");
        release_upload(
            &mut app,
            &UploadChunks::new(&chunk_root),
            UploadInput {
                upload_id: upload.upload_id,
            },
        )
        .unwrap();
        assert_eq!(app.upload_quota("a").unwrap().project_reserved_bytes, "0");
    }
}
