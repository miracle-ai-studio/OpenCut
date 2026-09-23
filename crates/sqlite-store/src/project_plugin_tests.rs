use super::*;
use opencut_editor_application::{
    access::{PrincipalPolicy, ProjectGrant},
    audit::{CallSource, ExecutionContext},
    project_plugins::*,
};
fn setup(path: &Path) -> (EditorApplication, SetGrant) {
    let mut app = EditorApplication::open(path).unwrap();
    app.create_project(
        CreateProject {
            id: "a".into(),
            name: "A".into(),
            sequence_id: "main".into(),
            width: 64,
            height: 48,
            fps: opencut_contracts::FrameRate {
                numerator: 10,
                denominator: 1,
            },
        },
        "a",
    )
    .unwrap();
    let input = SetGrant {
        target: PluginTarget {
            project_id: "a".into(),
            id: "org.example.planner".into(),
            version: "1.0.0".into(),
        },
        package_sha256: "a".repeat(64),
        operations: ["org.example.planner.plan".into()].into(),
        enabled: true,
        expected_revision: "0".into(),
        idempotency_key: "grant".into(),
    };
    (app, input)
}
#[test]
fn grant_atomic_receipt_cas_and_audit_failure() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, input) = setup(&dir.path().join("db"));
    let head = app.snapshot("a").unwrap();
    let grant = app.set_plugin_grant(&input).unwrap();
    assert_eq!(grant.revision, "1");
    assert_eq!(app.set_plugin_grant(&input).unwrap(), grant);
    let mut changed = input.clone();
    changed.enabled = false;
    assert_eq!(
        app.set_plugin_grant(&changed).unwrap_err().code,
        "IDEMPOTENCY_KEY_REUSED"
    );
    changed.idempotency_key = "next".into();
    assert_eq!(
        app.set_plugin_grant(&changed).unwrap_err().code,
        "REVISION_CONFLICT"
    );
    changed.expected_revision = "1".into();
    app.connection.execute_batch("CREATE TRIGGER fail_audit BEFORE INSERT ON audit_records BEGIN SELECT RAISE(ABORT,'test'); END;").unwrap();
    assert_eq!(
        app.set_plugin_grant(&changed).unwrap_err().code,
        "STORAGE_ERROR"
    );
    assert_eq!(app.plugin_grants("a").unwrap(), vec![grant.clone()]);
    assert_eq!(
        app.connection
            .query_row::<i64, _, _>(
                "SELECT count(*) FROM outbox WHERE event_type='project.plugin_grant.updated'",
                [],
                |r| r.get(0)
            )
            .unwrap(),
        1
    );
    app.connection
        .execute_batch("DROP TRIGGER fail_audit;")
        .unwrap();
    assert_eq!(app.set_plugin_grant(&changed).unwrap().revision, "2");
    assert_eq!(app.set_plugin_grant(&input).unwrap(), grant);
    assert_eq!(app.snapshot("a").unwrap(), head);
}
#[test]
fn revalidate_fences_policy_aba_and_credentials() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db");
    let (mut owner, input) = setup(&db);
    owner.set_plugin_grant(&input).unwrap();
    let mut policy = PrincipalPolicy {
        principal_id: "agent".into(),
        expected_revision: "0".into(),
        enabled: true,
        grants: vec![ProjectGrant {
            project_id: "a".into(),
            actions: vec![
                ProjectAction::Read,
                ProjectAction::Edit,
                ProjectAction::RunJobs,
            ],
        }],
    };
    owner.set_principal_policy(policy.clone()).unwrap();
    let credential = owner.issue_credential("agent").unwrap();
    let mut agent = EditorApplication::open(&db).unwrap();
    agent.set_execution_context(ExecutionContext::for_caller(
        CallSource::Cli,
        owner.authenticate(&credential.secret).unwrap(),
    ));
    let inspect = InspectInput {
        target: input.target.clone(),
        operation_name: "org.example.planner.plan".into(),
    };
    let ticket = agent.authorize_plugin(&inspect, true).unwrap();
    policy.expected_revision = "1".into();
    policy.enabled = false;
    owner.set_principal_policy(policy.clone()).unwrap();
    assert_eq!(
        agent
            .revalidate_plugin(&inspect, true, &ticket)
            .unwrap_err()
            .code,
        "NOT_AUTHORIZED"
    );
    policy.expected_revision = "2".into();
    policy.enabled = true;
    owner.set_principal_policy(policy).unwrap();
    assert_eq!(
        agent
            .revalidate_plugin(&inspect, true, &ticket)
            .unwrap_err()
            .code,
        "NOT_AUTHORIZED"
    );
    assert!(agent.authorize_plugin(&inspect, true).is_ok());
    assert_eq!(
        agent.set_plugin_grant(&input).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
    let ticket = agent.authorize_plugin(&inspect, true).unwrap();
    let mut next = input;
    next.expected_revision = "1".into();
    next.idempotency_key = "next".into();
    owner.set_plugin_grant(&next).unwrap();
    assert_eq!(
        agent
            .revalidate_plugin(&inspect, true, &ticket)
            .unwrap_err()
            .code,
        "NOT_AUTHORIZED"
    );
}
#[test]
fn concurrent_grant_updates_only_one_commits() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db");
    let (mut app, input) = setup(&db);
    app.set_plugin_grant(&input).unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
    let mut handles = vec![];
    for i in 0..2 {
        let db = db.clone();
        let barrier = barrier.clone();
        let mut next = input.clone();
        next.expected_revision = "1".into();
        next.idempotency_key = format!("next-{i}");
        handles.push(std::thread::spawn(move || {
            let mut app = EditorApplication::open(db).unwrap();
            barrier.wait();
            app.set_plugin_grant(&next)
        }));
    }
    barrier.wait();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| r.as_ref().is_err_and(|e| e.code == "REVISION_CONFLICT"))
            .count(),
        1
    );
}

#[test]
fn application_discards_output_after_runtime_revocation() {
    use opencut_api_contracts::plugins::PlanInput;
    use std::cell::RefCell;
    struct Runtime(RefCell<Option<Box<dyn FnOnce()>>>);
    impl ProjectPluginRuntime for Runtime {
        fn inspect_project_operation(
            &self,
            _: &InspectInput,
            _: &Grant,
            _: &dyn Fn() -> Result<(), EditorError>,
        ) -> Result<OperationView, EditorError> {
            unreachable!()
        }
        fn propose_project_operations(
            &self,
            _: &PlanInput,
            _: &ProjectSnapshot,
            _: &Grant,
            authorize: &dyn Fn() -> Result<(), EditorError>,
        ) -> Result<Vec<opencut_contracts::Operation>, EditorError> {
            authorize()?;
            self.0.borrow_mut().take().unwrap()();
            Ok(vec![])
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db");
    let (mut app, input) = setup(&db);
    app.set_plugin_grant(&input).unwrap();
    let snapshot = app.snapshot("a").unwrap();
    let request = PlanInput {
        id: input.target.id.clone(),
        version: input.target.version.clone(),
        operation_name: "org.example.planner.plan".into(),
        project_id: "a".into(),
        revision: snapshot.revision.clone(),
        parameters: json!({}),
    };
    let runtime = Runtime(RefCell::new(Some(Box::new(move || {
        let mut owner = EditorApplication::open(db).unwrap();
        let next = SetGrant {
            enabled: false,
            expected_revision: "1".into(),
            idempotency_key: "revoke".into(),
            ..input
        };
        owner.set_plugin_grant(&next).unwrap();
    }))));
    assert_eq!(
        opencut_editor_application::project_plugins::plan(&app, &runtime, request)
            .unwrap_err()
            .code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(app.snapshot("a").unwrap(), snapshot);
}
