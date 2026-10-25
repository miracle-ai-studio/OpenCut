use super::*;
use futures_util::StreamExt;
use opencut_contracts::project_resources::*;
use opencut_editor_application::access::{PrincipalPolicy, ProjectAction, ProjectGrant};
use opencut_host_runtime::workspace::Workspace;

#[test]
fn reused_dispatch_connection_resets_caller_and_observes_live_revocation() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("workspace.sqlite");
    let connections = AutomationConnections::default();
    let owner = Identity::owner();
    let created = connections.call(&database, &owner, "opencut_project_create", json!({
        "project":{"id":"p","name":"Fixture","sequence_id":"main","width":64,"height":48,"fps":{"numerator":25,"denominator":1}},
        "idempotency_key":"create"
    })).unwrap();
    assert!(created.error.is_none(), "{:?}", created.error);
    let mut admin = Workspace::open_managed(&database).unwrap();
    for (principal, grants) in [
        (
            "reader",
            vec![ProjectGrant {
                project_id: "p".into(),
                actions: vec![ProjectAction::Read],
            }],
        ),
        ("outsider", vec![]),
    ] {
        admin
            .set_principal_policy(PrincipalPolicy {
                principal_id: principal.into(),
                expected_revision: "0".into(),
                enabled: true,
                grants,
            })
            .unwrap();
    }
    let reader_credential = admin.issue_credential("reader").unwrap();
    let reader = Identity::authenticate(&database, &reader_credential.secret).unwrap();
    let outsider_credential = admin.issue_credential("outsider").unwrap();
    let outsider = Identity::authenticate(&database, &outsider_credential.secret).unwrap();
    let inspect = |identity: &Identity| {
        connections
            .call(
                &database,
                identity,
                "opencut_project_inspect",
                json!({"project_id":"p"}),
            )
            .unwrap()
    };
    assert!(inspect(&reader).error.is_none());
    assert_eq!(inspect(&outsider).error.unwrap().code, "NOT_AUTHORIZED");
    assert!(inspect(&owner).error.is_none());
    assert!(inspect(&reader).error.is_none());
    // The caller object and connection are unchanged; a separate writer revokes it.
    admin
        .revoke_credential(&reader_credential.credential_id)
        .unwrap();
    assert_eq!(inspect(&reader).error.unwrap().code, "NOT_AUTHORIZED");
    assert!(inspect(&owner).error.is_none());
    assert_eq!(connections.0.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn fixed_resource_stream_stops_after_revoke_and_after_owner_deletion() {
    for revoke in [true, false] {
        let directory = tempfile::tempdir().unwrap();
        let database = Arc::new(directory.path().join("workspace.sqlite"));
        let mut automation = Automation::open(database.as_ref()).unwrap();
        let created = automation.call("opencut_project_create", json!({"project":{"id":"p","name":"Fixture","sequence_id":"main","width":64,"height":48,"fps":{"numerator":25,"denominator":1}},"idempotency_key":"create"}));
        let revision = created.data.unwrap()["revision"]
            .as_str()
            .unwrap()
            .to_owned();
        let mut app = Workspace::open_managed(database.as_ref()).unwrap();
        let (identity, credential) = if revoke {
            app.set_principal_policy(PrincipalPolicy {
                principal_id: "reader".into(),
                expected_revision: "0".into(),
                enabled: true,
                grants: vec![ProjectGrant {
                    project_id: "p".into(),
                    actions: vec![ProjectAction::Read],
                }],
            })
            .unwrap();
            let issued = app.issue_credential("reader").unwrap();
            (
                Identity::authenticate(&database, &issued.secret).unwrap(),
                Some(issued.credential_id),
            )
        } else {
            (Identity::owner(), None)
        };
        let host = Host {
            database: database.clone(),
            automation: Arc::new(AutomationConnections::default()),
            token: Arc::new("fixture".into()),
            browser_session: None,
            origins: Arc::new(vec![]),
            media_root: Arc::new(directory.path().join("media")),
            artifact_root: Arc::new(directory.path().join("artifacts")),
            shutdown: Arc::new(AtomicBool::new(false)),
            identity,
            upload_slots: Arc::new(tokio::sync::Semaphore::new(1)),
        };
        let first = vec![42_u8; 64 * 1024];
        let body = axum::body::Body::from_stream(futures_util::stream::iter([
            Ok::<_, std::io::Error>(first.clone()),
            Ok(b"second".to_vec()),
        ]));
        let mut response = HttpResponse::new(body);
        response
            .extensions_mut()
            .insert(crate::http_identity::RevalidateStream);
        // The handler has already selected/verified bytes. This fixture
        // isolates subsequent live authorization from immutable selection.
        response
            .extensions_mut()
            .insert(crate::http_identity::RevalidateProjectResource(
                InspectResource {
                    project_id: "p".into(),
                    revision: revision.clone(),
                    kind: ResourceKind::Asset,
                    resource_id: "selected".into(),
                },
            ));
        let guarded = crate::http_identity::guard_stream(
            &host,
            response,
            opencut_automation::CallSource::Http,
        );
        let mut chunks = guarded.into_body().into_data_stream();
        assert_eq!(chunks.next().await.unwrap().unwrap().as_ref(), first);
        if let Some(credential) = credential {
            app.revoke_credential(&credential).unwrap();
        } else {
            let removed=automation.call("opencut_project_delete",json!({"projects":[{"project_id":"p","expected_revision":revision}],"idempotency_key":"delete"}));
            assert!(removed.error.is_none(), "{:?}", removed.error);
        }
        assert!(
            chunks.next().await.is_none(),
            "changed scope must stop before next bytes"
        );
    }
}
