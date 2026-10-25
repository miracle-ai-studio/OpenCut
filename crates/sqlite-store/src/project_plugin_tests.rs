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
fn planner_reuses_grant_fences_public_projection_and_atomic_dry_run() {
    use opencut_contracts::{project_public::*, scene_ops::*};
    use opencut_editor_application::project_plugins as planning;
    use std::cell::{Cell, RefCell};
    struct Runtime {
        observed: Cell<bool>,
        revoke: RefCell<Option<Box<dyn FnOnce()>>>,
        invalid: bool,
    }
    impl planning::ProjectPluginRuntime for Runtime {
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
            _: &opencut_api_contracts::plugins::PlanInput,
            snapshot: &PublicProjectSnapshot,
            _: &Grant,
            authorize: &dyn Fn() -> Result<(), EditorError>,
        ) -> Result<Vec<ProjectEditOperation>, EditorError> {
            authorize()?;
            assert!(
                serde_json::to_value(&snapshot.project)
                    .unwrap()
                    .get("schema_version")
                    .is_none()
            );
            self.observed.set(true);
            if let Some(revoke) = self.revoke.borrow_mut().take() {
                revoke();
            }
            Ok(if self.invalid {
                vec![ProjectEditOperation::TrackDelete {
                    sequence_id: "missing".into(),
                    track_id: "missing".into(),
                }]
            } else {
                vec![ProjectEditOperation::Rename {
                    name: "Plugin proposed".into(),
                }]
            })
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db");
    let (mut app, grant) = setup(&db);
    let snapshot = app
        .create_scene_project(&CreateProjectRequest {
            project: CreateProject {
                id: "v4".into(),
                name: "V4".into(),
                sequence_id: "main".into(),
                width: 64,
                height: 48,
                fps: opencut_contracts::FrameRate {
                    numerator: 10,
                    denominator: 1,
                },
            },
            idempotency_key: "v4-create".into(),
        })
        .unwrap();
    let mut grant = grant;
    grant.target.project_id = "v4".into();
    app.set_plugin_grant(&grant).unwrap();
    let input = planning::PlanProjectPlugin {
        branch_name: None,
        plugin: opencut_api_contracts::plugins::PlanInput {
            project_id: "v4".into(),
            revision: snapshot.revision.clone(),
            id: grant.target.id.clone(),
            version: grant.target.version.clone(),
            operation_name: grant.operations.iter().next().unwrap().clone(),
            parameters: serde_json::json!({}),
        },
        idempotency_key: "v4-plugin-apply".into(),
        label: "Reviewed plugin proposal".into(),
    };
    let runtime = Runtime {
        observed: Cell::new(false),
        revoke: RefCell::new(None),
        invalid: false,
    };
    let plan = planning::plan(&app, &runtime, input.clone()).unwrap();
    assert!(runtime.observed.get());
    assert_eq!(plan.preview.name, "Plugin proposed");
    assert_eq!(plan.transaction.expected_revision, snapshot.revision);
    assert_eq!(
        app.inspect_scene_project(&InspectProject {
            project_id: "v4".into(),
            revision: None
        })
        .unwrap(),
        snapshot
    );
    let invalid = Runtime {
        observed: Cell::new(false),
        revoke: RefCell::new(None),
        invalid: true,
    };
    assert!(planning::plan(&app, &invalid, input.clone()).is_err());
    assert_eq!(
        app.inspect_scene_project(&InspectProject {
            project_id: "v4".into(),
            revision: None
        })
        .unwrap(),
        snapshot
    );
    let revoke_db = db.clone();
    let revoke = Runtime {
        observed: Cell::new(false),
        invalid: false,
        revoke: RefCell::new(Some(Box::new(move || {
            let mut app = EditorApplication::open(&revoke_db).unwrap();
            let mut disabled = grant;
            disabled.enabled = false;
            disabled.expected_revision = "1".into();
            disabled.idempotency_key = "revoke-v4".into();
            app.set_plugin_grant(&disabled).unwrap();
        }))),
    };
    assert_eq!(
        planning::plan(&app, &revoke, input.clone())
            .unwrap_err()
            .code,
        "NOT_AUTHORIZED"
    );
    runtime.observed.set(false);
    assert_eq!(
        planning::plan(&app, &runtime, input).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
    assert!(!runtime.observed.get());
    assert_eq!(
        app.inspect_scene_project(&InspectProject {
            project_id: "v4".into(),
            revision: None
        })
        .unwrap(),
        snapshot
    );
}
