use super::*;
use opencut_editor_application::{access::*, projects::*};

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
fn policy(id: &str, revision: &str, actions: Vec<ProjectAction>) -> PrincipalPolicy {
    PrincipalPolicy {
        principal_id: id.into(),
        expected_revision: revision.into(),
        enabled: true,
        grants: vec![ProjectGrant {
            project_id: "a".into(),
            actions,
        }],
    }
}
fn scoped(app: &mut EditorApplication, actions: Vec<ProjectAction>) -> (Caller, IssuedCredential) {
    app.set_principal_policy(policy("agent", "0", actions))
        .unwrap();
    let issued = app.issue_credential("agent").unwrap();
    (app.authenticate(&issued.secret).unwrap(), issued)
}
fn edit(revision: &str, key: &str) -> TransactionRequest {
    TransactionRequest {
        expected_revision: revision.into(),
        idempotency_key: key.into(),
        label: key.into(),
        operations: vec![opencut_contracts::Operation::UpdateSequenceSettings {
            sequence_id: "main".into(),
            name: Some(key.into()),
            width: None,
            height: None,
            fps: None,
        }],
    }
}
fn job(snapshot: &ProjectSnapshot, key: &str) -> JobRequest {
    JobRequest {
        idempotency_key: key.into(),
        kind: "preview.composited_frame".into(),
        input: json!({"snapshot":snapshot,"sequence_id":"main","requested_time":{"ticks":"0","timebase":10},"alignment":"exact"}),
    }
}
fn denied<T>(result: Result<T, EditorError>) {
    assert!(matches!(result,Err(e) if e.code=="NOT_AUTHORIZED"));
}
fn counts(app: &EditorApplication) -> Vec<i64> {
    [
        "projects",
        "revisions",
        "editor_transactions",
        "idempotency",
        "outbox",
        "jobs",
        "job_events",
        "job_projects",
        "audit_records",
    ]
    .iter()
    .map(|t| {
        app.connection
            .query_row(&format!("SELECT count(*) FROM {t}"), [], |r| r.get(0))
            .unwrap()
    })
    .collect()
}
#[test]
fn administration_history_is_owner_only_and_pages_large_snapshots_without_skipping() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    project(&mut app, "a");
    let (caller, issued) = scoped(
        &mut app,
        vec![ProjectAction::Read, ProjectAction::ReadAudit],
    );
    let query = AccessAuditQuery {
        target: AccessAuditTarget::Workspace,
        after_sequence: "0".into(),
        limit: 1,
    };
    let first = app.access_audit_query(query.clone()).unwrap();
    assert!(first.has_more);
    assert_eq!(first.records[0].action, AccessAuditAction::PolicyUpdated);
    assert_eq!(first.records[0].policy.as_ref().unwrap().revision, "1");
    let mut next = query.clone();
    next.after_sequence = first.last_sequence.clone();
    let second = app.access_audit_query(next).unwrap();
    assert!(!second.has_more);
    assert_eq!(
        second.records[0].credential_id.as_deref(),
        Some(issued.credential_id.as_str())
    );
    app.set_caller(caller);
    denied(app.access_audit_query(query.clone()));
    // Even a trusted caller argument cannot bypass the adapter's scoped context.
    denied(opencut_editor_application::access_audit::query(
        &app.connection,
        &Caller::LocalOwner,
        query.clone(),
    ));
    app.set_caller(Caller::LocalOwner);
    let large = PrincipalView {
        principal_id: "historical".into(),
        revision: "1".into(),
        enabled: true,
        grants: (0..1000)
            .map(|n| ProjectGrant {
                project_id: format!("{}-{n}", "\\".repeat(1000)),
                actions: vec![ProjectAction::Read],
            })
            .collect(),
    };
    // Stored historical snapshots deliberately exercise worst-case JSON escaping.
    let payload = serde_json::to_string(&large).unwrap();
    for n in 0..3 {
        app.connection.execute("INSERT INTO access_events(principal_id,action,request_id,actor_json,policy_json) VALUES ('historical','policy.updated',?1,?2,?3)",
            params![format!("large-{n}"),serde_json::to_string(app.connection.1.actor()).unwrap(),payload]).unwrap();
    }
    let mut large_query = AccessAuditQuery {
        target: AccessAuditTarget::Principal("historical".into()),
        after_sequence: "0".into(),
        limit: 500,
    };
    let mut sequences = vec![];
    loop {
        let page = app.access_audit_query(large_query.clone()).unwrap();
        assert_eq!(page.records.len(), 1);
        assert!(serde_json::to_vec(&page).unwrap().len() < 4_194_304);
        sequences.push(page.records[0].sequence.clone());
        large_query.after_sequence = page.last_sequence;
        if !page.has_more {
            break;
        }
    }
    assert_eq!(sequences, ["3", "4", "5"]);
    assert!(
        app.access_audit_query(large_query)
            .unwrap()
            .records
            .is_empty()
    );
}
#[test]
fn policy_revisions_credentials_and_administration_are_atomic() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    project(&mut app, "a");
    let (caller, issued) = scoped(&mut app, vec![ProjectAction::Read]);
    let second = app.issue_credential("agent").unwrap();
    let stored: String = app
        .connection
        .query_row(
            "SELECT secret_digest FROM access_credentials WHERE credential_id=?1",
            [&issued.credential_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stored.len(), 64);
    assert_ne!(stored, issued.secret);
    assert!(
        matches!(app.set_principal_policy(policy("agent","0",vec![ProjectAction::Read])),Err(e) if e.code=="REVISION_CONFLICT")
    );
    app.connection.execute_batch("CREATE TRIGGER deny_policy BEFORE INSERT ON access_events BEGIN SELECT RAISE(ABORT,'policy audit failed'); END;").unwrap();
    assert!(
        app.set_principal_policy(policy(
            "agent",
            "1",
            vec![ProjectAction::Read, ProjectAction::Edit]
        ))
        .is_err()
    );
    assert_eq!(app.principal_policy("agent").unwrap().revision, "1");
    assert_eq!(
        app.principal_policy("agent").unwrap().grants[0].actions,
        vec![ProjectAction::Read]
    );
    assert!(app.revoke_credential(&issued.credential_id).is_err());
    assert!(app.authenticate(&issued.secret).is_ok());
    app.connection
        .execute_batch("DROP TRIGGER deny_policy;")
        .unwrap();
    app.revoke_credential(&issued.credential_id).unwrap();
    app.revoke_credential(&issued.credential_id).unwrap();
    denied(app.authenticate(&issued.secret));
    assert!(app.authenticate(&second.secret).is_ok());
    app.set_caller(caller);
    denied(app.access_view());
    denied(app.set_principal_policy(policy("agent", "1", vec![ProjectAction::Read])));
    denied(app.issue_credential("agent"));
    app.set_caller(Caller::LocalOwner);
    let mut disabled = policy("agent", "1", vec![ProjectAction::Read]);
    disabled.enabled = false;
    app.set_principal_policy(disabled).unwrap();
    denied(app.authenticate(&second.secret));
}
#[test]
fn reads_and_receipts_follow_project_scope_and_current_grants() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let a = project(&mut app, "a");
    let b = project(&mut app, "b");
    let request = edit(&a.revision, "owner-edit");
    let original = app.apply("a", &request).unwrap();
    let (caller, _) = scoped(&mut app, vec![ProjectAction::Read]);
    app.set_caller(caller.clone());
    assert_eq!(app.list_projects().unwrap().len(), 1);
    assert_eq!(app.snapshot("a").unwrap().revision, original.revision);
    denied(app.snapshot("b"));
    denied(app.snapshot("missing"));
    denied(app.snapshot_at("b", &b.revision));
    denied(app.plan("a", &request));
    denied(projects::plan(&app, "a", &request));
    denied(app.apply("a", &request));
    denied(app.create_project(
        CreateProject {
            id: "a".into(),
            name: "a".into(),
            sequence_id: "main".into(),
            width: 64,
            height: 48,
            fps: opencut_contracts::FrameRate {
                numerator: 10,
                denominator: 1,
            },
        },
        "create-a",
    ));
    assert!(
        app.events_after(0, 500)
            .unwrap()
            .iter()
            .all(|e| e.project_id == "a")
    );
    let query = opencut_api_contracts::audit::AuditQuery {
        target: opencut_api_contracts::audit::AuditTarget::Workspace,
        after_sequence: "0".into(),
        limit: 100,
    };
    assert!(app.audit_query(query.clone()).unwrap().records.is_empty());
    app.set_caller(Caller::LocalOwner);
    app.set_principal_policy(policy(
        "agent",
        "1",
        vec![
            ProjectAction::Read,
            ProjectAction::Edit,
            ProjectAction::ReadAudit,
        ],
    ))
    .unwrap();
    app.set_caller(caller);
    assert_eq!(app.apply("a", &request).unwrap(), original);
    assert!(
        app.audit_query(query)
            .unwrap()
            .records
            .iter()
            .all(|r| r.project_id.as_deref() == Some("a"))
    );
    assert_eq!(app.history("a", None, 100).unwrap().revisions.len(), 2);
    denied(app.history("b", None, 100));
}
#[test]
fn task_membership_pages_events_and_idempotency_cannot_cross_projects() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let a = project(&mut app, "a");
    let b = project(&mut app, "b");
    let first = app.create_job(job(&a, "first")).unwrap();
    let hidden = app.create_job(job(&b, "hidden-key")).unwrap();
    let forged = {
        let mut r = job(&a, "forged");
        r.input["snapshot"]["project"]["name"] = json!("forged");
        app.create_job(r).unwrap()
    };
    let last = app.create_job(job(&a, "last")).unwrap();
    let (caller, issued) = scoped(
        &mut app,
        vec![
            ProjectAction::Read,
            ProjectAction::RunJobs,
            ProjectAction::CancelJobs,
        ],
    );
    app.set_caller(caller.clone());
    assert_eq!(app.list_jobs().unwrap().len(), 2);
    denied(app.get_job(&hidden.job_id));
    denied(app.get_job(&forged.job_id));
    denied(app.find_existing_job(&job(&a, "hidden-key")));
    denied(app.create_job(job(&a, "hidden-key")));
    assert_eq!(
        app.create_job(job(&a, "first")).unwrap().job_id,
        first.job_id
    );
    let slice = queries::JobRead::read_job_page(&app.connection, None, None, 1).unwrap();
    assert!(slice.has_more);
    assert_eq!(slice.upper_id.as_deref(), Some(last.job_id.as_str()));
    assert_eq!(slice.records[0].job_id, first.job_id);
    assert!(
        matches!(queries::JobRead::read_job_page(&app.connection,Some(&hidden.job_id),None,1),Err(e) if e.code=="CURSOR_EXPIRED")
    );
    let events = app.job_events_after(0, 500, None).unwrap();
    assert_eq!(events.len(), 2);
    assert!(
        events
            .iter()
            .all(|e| e.job_id == first.job_id || e.job_id == last.job_id)
    );
    denied(app.job_events_after(0, 100, Some(&hidden.job_id)));
    denied(app.cancel_job(&hidden.job_id));
    assert_eq!(
        app.cancel_job(&first.job_id).unwrap().state,
        JobState::Cancelled
    );
    denied(app.claim_job("worker", 0, 100));
    denied(app.recover_expired(100));
    let request = job(&a, "revoked-preparation");
    assert!(app.find_existing_job(&request).unwrap().is_none());
    app.set_caller(Caller::LocalOwner);
    app.revoke_credential(&issued.credential_id).unwrap();
    let before = counts(&app);
    app.set_caller(caller);
    denied(app.create_job(request));
    denied(app.create_job(job(&a, "first")));
    denied(app.cancel_job(&first.job_id));
    denied(app.list_projects());
    denied(app.list_jobs());
    assert_eq!(counts(&app), before);
}
struct RevokeBeforeCommit {
    db: SqliteConnection,
    owner: EditorApplication,
    credential: String,
}
impl ProjectRead for RevokeBeforeCommit {
    fn authorize_edit(&self, project_id: &str, intent: EditIntent<'_>) -> Result<(), EditorError> {
        self.db.authorize_edit(project_id, intent)
    }
    fn head(&self, id: &str) -> Result<ProjectSnapshot, EditorError> {
        self.db.head(id)
    }
    fn revision(&self, id: &str, rev: &str) -> Result<ProjectSnapshot, EditorError> {
        self.db.revision(id, rev)
    }
    fn replay_create(
        &self,
        input: &CreateProject,
        key: &str,
    ) -> Result<Option<ProjectSnapshot>, EditorError> {
        self.db.replay_create(input, key)
    }
    fn replay_edit(
        &self,
        id: &str,
        intent: EditIntent<'_>,
    ) -> Result<Option<TransactionResult>, EditorError> {
        self.db.replay_edit(id, intent)
    }
}
impl ProjectStore for RevokeBeforeCommit {
    fn create_atomic(
        &mut self,
        _: &CreateProject,
        _: &str,
        _: ProjectSnapshot,
    ) -> Result<ProjectSnapshot, EditorError> {
        panic!("unexpected create")
    }
    fn commit_atomic(
        &mut self,
        intent: EditIntent<'_>,
        prepared: PreparedCommit,
    ) -> Result<TransactionResult, EditorError> {
        self.owner.revoke_credential(&self.credential).unwrap();
        self.db.commit_atomic(intent, prepared)
    }
}
struct NoAssets;
impl AssetPinning for NoAssets {
    fn pin(
        &mut self,
        _: &opencut_contracts::Asset,
    ) -> Result<opencut_contracts::Asset, EditorError> {
        panic!("unexpected asset capture")
    }
}
#[test]
fn revoke_between_plan_and_commit_prevents_all_business_writes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("db");
    let mut owner = EditorApplication::open(&path).unwrap();
    let a = project(&mut owner, "a");
    let (caller, issued) = scoped(&mut owner, vec![ProjectAction::Read, ProjectAction::Edit]);
    let mut app = EditorApplication::open(&path).unwrap();
    app.set_caller(caller);
    let before = counts(&app);
    let mut store = RevokeBeforeCommit {
        db: app.connection,
        owner,
        credential: issued.credential_id,
    };
    denied(projects::apply(
        &mut store,
        &mut NoAssets,
        "a",
        &edit(&a.revision, "race"),
    ));
    assert_eq!(store.owner.snapshot("a").unwrap(), a);
    assert_eq!(counts(&store.owner), before);
}
#[test]
fn migration_only_adopts_jobs_with_authoritative_snapshots_and_rolls_back_on_collision() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let (valid, forged) = {
        let mut app = EditorApplication::open(&path).unwrap();
        let a = project(&mut app, "a");
        let valid = app.create_job(job(&a, "valid")).unwrap();
        let mut req = job(&a, "forged");
        req.input["snapshot"]["project"]["name"] = json!("forged");
        let forged = app.create_job(req).unwrap();
        (valid, forged)
    };
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(sqlite_artifact_publication::DROP_FOR_TEST)
        .unwrap();
    connection.execute_batch("DROP TABLE artifact_gc_plans; DROP TABLE artifact_retention; DROP TABLE cas_gc_events; DROP TABLE cas_gc_plans; DROP TABLE denial_records; DROP TABLE project_plugin_grants; DROP TABLE upload_retention; DROP TABLE project_media; DROP TABLE upload_chunks; DROP TABLE upload_sessions; DROP TABLE plugin_events; DROP TABLE plugin_publications; DROP TABLE maintenance_events; DROP TABLE maintenance_publications; DROP TABLE job_projects; DROP TABLE access_events; DROP TABLE access_credentials; DROP TABLE access_grants; DROP TABLE access_principals; UPDATE metadata SET value='4'; CREATE TABLE job_projects(collision TEXT);").unwrap();
    assert!(EditorApplication::open(&path).is_err());
    assert_eq!(
        connection
            .query_row::<String, _, _>(
                "SELECT value FROM metadata WHERE key='schema_version'",
                [],
                |r| r.get(0)
            )
            .unwrap(),
        "4"
    );
    assert_eq!(
        connection
            .query_row::<i64, _, _>(
                "SELECT count(*) FROM sqlite_master WHERE name='access_principals'",
                [],
                |r| r.get(0)
            )
            .unwrap(),
        0
    );
    connection
        .execute_batch("DROP TABLE job_projects;")
        .unwrap();
    drop(connection);
    let app = EditorApplication::open(&path).unwrap();
    assert_eq!(
        sqlite_access::job_project(&app.connection, &valid.job_id)
            .unwrap()
            .as_deref(),
        Some("a")
    );
    assert!(
        sqlite_access::job_project(&app.connection, &forged.job_id)
            .unwrap()
            .is_none()
    );
}

#[test]
fn concurrent_policy_updates_have_one_winner_and_restore_is_a_distinct_permission() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut app = EditorApplication::open(&path).unwrap();
    let a = project(&mut app, "a");
    let (caller, _) = scoped(&mut app, vec![ProjectAction::Read, ProjectAction::Edit]);
    let revision = app
        .apply("a", &edit(&a.revision, "rename"))
        .unwrap()
        .revision;
    let restore = RestoreRevisionRequest {
        expected_revision: revision,
        idempotency_key: "restore".into(),
        target_revision: a.revision,
        label: "restore".into(),
    };
    app.set_caller(caller.clone());
    denied(app.restore_revision("a", &restore));
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let path = path.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let mut owner = EditorApplication::open(path).unwrap();
                barrier.wait();
                owner.set_principal_policy(policy(
                    "agent",
                    "1",
                    vec![ProjectAction::Read, ProjectAction::Restore],
                ))
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| r.as_ref().is_err_and(|e| e.code == "REVISION_CONFLICT"))
            .count(),
        1
    );
    let restored = app.restore_revision("a", &restore).unwrap();
    assert_eq!(app.restore_revision("a", &restore).unwrap(), restored);
    denied(app.apply("a", &edit(&restored.revision, "edit-after-policy")));
}
