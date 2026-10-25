//! Persistent owner edit hold, checked by project mutation transactions.
pub(super) const MIGRATION: &str = "
CREATE TABLE project_edit_control (
 project_id TEXT PRIMARY KEY REFERENCES projects(project_id),
 generation INTEGER NOT NULL CHECK(generation >= 1),
 held INTEGER NOT NULL CHECK(held IN (0,1)),
 held_by_principal TEXT
);
UPDATE metadata SET value='21' WHERE key='schema_version' AND value='20';
";

use super::*;
use opencut_api_contracts::edit_control::{EditControlRead, EditControlSet, EditControlView};

fn state(connection: &Connection, project: &str) -> Result<EditControlView, EditorError> {
    let row: Option<(i64, bool, Option<String>)> = connection
        .query_row(
            "SELECT generation,held,held_by_principal FROM project_edit_control WHERE project_id=?1",
            [project],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(storage_error)?;
    let (generation, held, held_by_principal) = row.unwrap_or((0, false, None));
    Ok(EditControlView {
        project_id: project.into(),
        generation: generation.to_string(),
        held,
        held_by_principal,
    })
}

pub(super) fn require_writable(
    connection: &Connection,
    context: &opencut_editor_application::audit::ExecutionContext,
    project: &str,
) -> Result<(), EditorError> {
    if !context.caller().is_owner() && state(connection, project)?.held {
        return Err(EditorError::new(
            "EDIT_HELD_BY_OWNER",
            "Project edits are reserved for the local owner",
        ));
    }
    Ok(())
}

impl EditorApplication {
    pub fn get_edit_control(
        &self,
        input: &EditControlRead,
    ) -> Result<EditControlView, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        sqlite_access::require(
            &tx,
            &self.connection.1,
            &input.project_id,
            ProjectAction::Read,
        )?;
        sqlite_project_document::project_head_revision(&tx, &input.project_id)?;
        let view = state(&tx, &input.project_id)?;
        tx.commit().map_err(storage_error)?;
        Ok(view)
    }

    pub fn set_edit_control(
        &mut self,
        input: &EditControlSet,
    ) -> Result<EditControlView, EditorError> {
        require_idempotency_key(&input.idempotency_key)?;
        let expected: i64 = input
            .expected_generation
            .parse()
            .map_err(|_| EditorError::new("INVALID_ARGUMENT", "Invalid edit-control generation"))?;
        if expected < 0 || expected.to_string() != input.expected_generation || expected == i64::MAX
        {
            return Err(EditorError::new(
                "INVALID_ARGUMENT",
                "Invalid edit-control generation",
            ));
        }
        let identity = request_hash("edit_control_set", &input.project_id, input)?;
        let context = self.connection.1.clone();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        sqlite_access::owner(&context)?;
        if let Some(value) = load_idempotent(
            &tx,
            "edit_control_set",
            &input.project_id,
            &input.idempotency_key,
            &identity,
            "edit_control",
        )? {
            return decode_json(&value, "edit-control receipt");
        }
        let head_revision = sqlite_project_document::project_head_revision(&tx, &input.project_id)?;
        let current = state(&tx, &input.project_id)?;
        if current.generation != input.expected_generation {
            return Err(EditorError::new(
                "REVISION_CONFLICT",
                "Edit-control generation changed",
            ));
        }
        let view = EditControlView {
            project_id: input.project_id.clone(),
            generation: (expected + 1).to_string(),
            held: input.held,
            held_by_principal: input
                .held
                .then(|| context.caller().principal_id().to_owned()),
        };
        tx.execute("INSERT INTO project_edit_control(project_id,generation,held,held_by_principal) VALUES (?1,?2,?3,?4)
            ON CONFLICT(project_id) DO UPDATE SET generation=excluded.generation,held=excluded.held,held_by_principal=excluded.held_by_principal",
            params![input.project_id,expected+1,input.held,view.held_by_principal]).map_err(storage_error)?;
        tx.execute("INSERT INTO idempotency(operation_family,scope,idempotency_key,request_hash,result_kind,result_json)
            VALUES ('edit_control_set',?1,?2,?3,'edit_control',?4)",
            params![input.project_id,input.idempotency_key,identity.current_digest(),encode_json(&view,"edit-control receipt")?]).map_err(idempotency_insert_error)?;
        tx.execute("INSERT INTO outbox(event_id,project_id,revision,event_type,payload_json) VALUES (?1,?2,?3,'project.edit_control_changed',?4)",
            params![format!("evt_{}",Uuid::new_v4().simple()),input.project_id,head_revision,encode_json(&view,"edit-control event")?]).map_err(storage_error)?;
        sqlite_audit::insert(
            &tx,
            &context,
            "project.edit_control_changed",
            sqlite_audit::Links {
                project_id: Some(&input.project_id),
                revision: Some(&head_revision),
                ..Default::default()
            },
        )?;
        tx.commit().map_err(storage_error)?;
        Ok(view)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencut_contracts::{CreateProject, FrameRate, TransactionRequest};
    use opencut_editor_application::access::{Caller, PrincipalPolicy, ProjectGrant};

    fn edit(revision: &str, key: &str, name: &str) -> TransactionRequest {
        serde_json::from_value(json!({"expected_revision":revision,"idempotency_key":key,
            "label":key,"operations":[{"type":"sequence.update_settings","sequence_id":"main",
            "name":name,"width":null,"height":null,"fps":null}]}))
        .unwrap()
    }

    #[test]
    fn owner_hold_survives_reopen_and_fences_scoped_mutations_but_not_plan() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("workspace.sqlite");
        let mut app = EditorApplication::open(&path).unwrap();
        let initial = app
            .create_project(
                CreateProject {
                    id: "p".into(),
                    name: "Project".into(),
                    sequence_id: "main".into(),
                    width: 640,
                    height: 360,
                    fps: FrameRate {
                        numerator: 25,
                        denominator: 1,
                    },
                },
                "create",
            )
            .unwrap();
        app.set_principal_policy(PrincipalPolicy {
            principal_id: "agent".into(),
            expected_revision: "0".into(),
            enabled: true,
            grants: vec![ProjectGrant {
                project_id: "p".into(),
                actions: vec![ProjectAction::Read, ProjectAction::Edit],
            }],
        })
        .unwrap();
        let secret = app.issue_credential("agent").unwrap();
        let caller = app.authenticate(&secret.secret).unwrap();
        let read = EditControlRead {
            project_id: "p".into(),
        };
        assert_eq!(app.get_edit_control(&read).unwrap().generation, "0");
        let hold = EditControlSet {
            project_id: "p".into(),
            expected_generation: "0".into(),
            held: true,
            idempotency_key: "hold".into(),
        };
        let held = app.set_edit_control(&hold).unwrap();
        assert_eq!(app.set_edit_control(&hold).unwrap(), held);
        assert_eq!(held.generation, "1");
        app.set_caller(caller.clone());
        assert_eq!(
            app.set_edit_control(&EditControlSet {
                idempotency_key: "agent-hold".into(),
                ..hold.clone()
            })
            .unwrap_err()
            .code,
            "NOT_AUTHORIZED"
        );
        assert!(
            app.plan("p", &edit(&initial.revision, "agent", "Agent"))
                .is_ok()
        );
        assert_eq!(
            app.apply("p", &edit(&initial.revision, "agent", "Agent"))
                .unwrap_err()
                .code,
            "EDIT_HELD_BY_OWNER"
        );
        assert_eq!(
            app.create_historical_branch(&opencut_api_contracts::branching::BranchCreateRequest {
                project_id: "p".into(),
                branch_name: "agent".into(),
                from_revision: initial.revision.clone(),
                expected_revision: initial.revision.clone(),
                idempotency_key: "agent-branch".into()
            })
            .unwrap_err()
            .code,
            "EDIT_HELD_BY_OWNER"
        );
        app.set_caller(Caller::LocalOwner);
        let owner = app
            .apply("p", &edit(&initial.revision, "owner", "Owner"))
            .unwrap();
        drop(app);
        let mut app = EditorApplication::open(&path).unwrap();
        assert_eq!(app.get_edit_control(&read).unwrap(), held);
        assert_eq!(
            app.set_edit_control(&EditControlSet {
                expected_generation: "0".into(),
                idempotency_key: "stale".into(),
                ..hold.clone()
            })
            .unwrap_err()
            .code,
            "REVISION_CONFLICT"
        );
        let released = app
            .set_edit_control(&EditControlSet {
                expected_generation: "1".into(),
                held: false,
                idempotency_key: "release".into(),
                ..hold
            })
            .unwrap();
        assert_eq!(released.generation, "2");
        assert_eq!(released.held_by_principal, None);
        app.set_caller(app.authenticate(&secret.secret).unwrap());
        assert!(
            app.apply("p", &edit(&owner.revision, "agent-after-release", "Agent"))
                .is_ok()
        );
    }

    #[test]
    fn edit_control_audit_failure_rolls_back_state_receipt_and_event() {
        let mut app = EditorApplication::open(":memory:").unwrap();
        app.create_project(
            CreateProject {
                id: "p".into(),
                name: "Project".into(),
                sequence_id: "main".into(),
                width: 640,
                height: 360,
                fps: FrameRate {
                    numerator: 25,
                    denominator: 1,
                },
            },
            "create",
        )
        .unwrap();
        let request = EditControlSet {
            project_id: "p".into(),
            expected_generation: "0".into(),
            held: true,
            idempotency_key: "hold".into(),
        };
        app.connection.execute_batch("CREATE TRIGGER fail_control_audit BEFORE INSERT ON audit_records
            WHEN NEW.action='project.edit_control_changed' BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
        assert_eq!(
            app.set_edit_control(&request).unwrap_err().code,
            "STORAGE_ERROR"
        );
        assert_eq!(
            app.get_edit_control(&EditControlRead {
                project_id: "p".into()
            })
            .unwrap()
            .generation,
            "0"
        );
        let receipt_count: i64 = app
            .connection
            .query_row(
                "SELECT COUNT(*) FROM idempotency
            WHERE operation_family='edit_control_set'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let event_count: i64 = app
            .connection
            .query_row(
                "SELECT COUNT(*) FROM outbox
            WHERE event_type='project.edit_control_changed'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!((receipt_count, event_count), (0, 0));
        app.connection
            .execute_batch("DROP TRIGGER fail_control_audit")
            .unwrap();
        assert_eq!(app.set_edit_control(&request).unwrap().generation, "1");
    }
}
