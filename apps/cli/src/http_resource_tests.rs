use super::*;
use futures_util::StreamExt;
use opencut_contracts::project_v4_resources::*;
use opencut_editor_application::access::{PrincipalPolicy, ProjectAction, ProjectGrant};
use opencut_host_runtime::workspace::Workspace;

#[tokio::test]
async fn fixed_resource_stream_stops_after_revoke_and_after_owner_deletion() {
    for revoke in [true, false] {
        let directory = tempfile::tempdir().unwrap();
        let database = Arc::new(directory.path().join("workspace.sqlite"));
        let mut automation = Automation::open(database.as_ref()).unwrap();
        let created = automation.call("opencut_project_v4_create", json!({"project":{"id":"p","name":"Fixture","sequence_id":"main","width":64,"height":48,"fps":{"numerator":25,"denominator":1}},"idempotency_key":"create"}));
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
            token: Arc::new("fixture".into()),
            browser_session: None,
            origins: Arc::new(vec![]),
            media_root: Arc::new(directory.path().join("media")),
            artifact_root: Arc::new(directory.path().join("artifacts")),
            shutdown: Arc::new(AtomicBool::new(false)),
            identity,
            upload_slots: Arc::new(tokio::sync::Semaphore::new(1)),
        };
        let body = axum::body::Body::from_stream(futures_util::stream::iter([
            Ok::<_, std::io::Error>(b"first".to_vec()),
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
            .insert(crate::http_identity::RevalidateV4Resource(
                InspectResourceV4 {
                    project_id: "p".into(),
                    revision: revision.clone(),
                    kind: ResourceKindV4::Asset,
                    resource_id: "selected".into(),
                },
            ));
        let guarded = crate::http_identity::guard_stream(
            &host,
            response,
            opencut_automation::CallSource::Http,
        );
        let mut chunks = guarded.into_body().into_data_stream();
        assert_eq!(chunks.next().await.unwrap().unwrap().as_ref(), b"first");
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
