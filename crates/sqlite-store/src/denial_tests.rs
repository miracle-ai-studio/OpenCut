use super::*;
use opencut_editor_application::{audit::CallSource, denials::*};
fn query() -> DenialQuery {
    DenialQuery {
        target: DenialTarget::Workspace,
        after_sequence: "0".into(),
        limit: 2,
    }
}
#[test]
fn duplicate_observations_keep_first_identity_without_consuming_sequence() {
    let app = EditorApplication::open(":memory:").unwrap();
    let id = Uuid::new_v4().to_string();
    let mut denial = Denial {
        request_id: &id,
        source: CallSource::Http,
        caller: None,
        operation: DeniedOperation::Authentication,
        reason: DenialReason::AuthenticationRejected,
    };
    app.record_denial(&denial).unwrap();
    denial.source = CallSource::Cli;
    app.record_denial(&denial).unwrap();
    let id2 = Uuid::new_v4().to_string();
    denial.request_id = &id2;
    app.record_denial(&denial).unwrap();
    let page = app.read_denials(&query()).unwrap();
    assert_eq!(page.records.len(), 2);
    assert_eq!(page.records[0].sequence, "1");
    assert_eq!(page.records[1].sequence, "2");
    assert_eq!(page.records[0].source, CallSource::Http);
    assert_eq!(page.records[0].principal_id, None);
}
#[test]
fn privacy_validation_owner_only_queries_and_retention_atomicity() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let id = Uuid::new_v4().to_string();
    let mut denial = Denial {
        request_id: &id,
        source: CallSource::Cli,
        caller: None,
        operation: DeniedOperation::Tool {
            name: "secret-unregistered-action".into(),
        },
        reason: DenialReason::AuthorizationDenied,
    };
    assert_eq!(
        app.record_denial(&denial).unwrap_err().code,
        "INVALID_ARGUMENT"
    );
    assert!(app.read_denials(&query()).unwrap().records.is_empty());
    denial.operation = DeniedOperation::UnknownTool;
    {
        let tx = app.connection.unchecked_transaction().unwrap();
        for _ in 0..MAX_RETAINED {
            tx.execute("INSERT INTO denial_records(request_id,source_json,operation_json,reason_json) VALUES (?1,'\"cli\"','{\"kind\":\"unknown_tool\"}','\"authorization_denied\"')",[Uuid::new_v4().to_string()]).unwrap();
        }
        tx.commit().unwrap();
    }
    app.connection.execute_batch("CREATE TRIGGER fail_prune BEFORE DELETE ON denial_records BEGIN SELECT RAISE(ABORT,'prune'); END").unwrap();
    assert_eq!(
        app.record_denial(&denial).unwrap_err().code,
        "STORAGE_ERROR"
    );
    assert_eq!(
        app.connection
            .query_row::<i64, _, _>(
                "SELECT COUNT(*) FROM denial_records WHERE request_id=?1",
                [&id],
                |r| r.get(0)
            )
            .unwrap(),
        0
    );
    app.connection
        .execute_batch("DROP TRIGGER fail_prune")
        .unwrap();
    app.record_denial(&denial).unwrap();
    let page = app.read_denials(&query()).unwrap();
    assert_eq!(page.oldest_available_sequence.as_deref(), Some("2"));
    assert_eq!(page.records.len(), 2);
    assert!(page.has_more);
    let invalid = DenialQuery {
        after_sequence: "01".into(),
        ..query()
    };
    assert_eq!(
        app.read_denials(&invalid).unwrap_err().code,
        "INVALID_ARGUMENT"
    );
    app.set_caller(access::Caller::Credential(
        access::VerifiedCredential::from_verified("not-owner".into(), "not-owner".into()),
    ));
    assert_eq!(
        app.read_denials(&query()).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
}
#[test]
fn concurrent_same_request_is_one_audit_record() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db");
    drop(EditorApplication::open(&db).unwrap());
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
    let id = Uuid::new_v4().to_string();
    let mut handles = vec![];
    for _ in 0..2 {
        let db = db.clone();
        let barrier = barrier.clone();
        let id = id.clone();
        handles.push(std::thread::spawn(move || {
            let app = EditorApplication::open(db).unwrap();
            barrier.wait();
            app.record_denial(&Denial {
                request_id: &id,
                source: CallSource::Http,
                caller: None,
                operation: DeniedOperation::Authentication,
                reason: DenialReason::AuthenticationRejected,
            })
        }));
    }
    barrier.wait();
    for h in handles {
        h.join().unwrap().unwrap();
    }
    assert_eq!(
        EditorApplication::open(db)
            .unwrap()
            .read_denials(&query())
            .unwrap()
            .records
            .len(),
        1
    );
}
