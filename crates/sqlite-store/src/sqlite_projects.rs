//! SQLite implementation of the project transaction port. SQL and locking stay here.
use super::projects::{AssetPinning, EditIntent, PreparedCommit, ProjectRead, ProjectStore};
use super::*;
use opencut_asset_store::AssetStore;
use opencut_editor_application::access::{self, ProjectAction};

fn authorize_edit(
    connection: &Connection,
    context: &opencut_editor_application::audit::ExecutionContext,
    project: &str,
    intent: EditIntent<'_>,
) -> Result<(), EditorError> {
    let action = match intent {
        EditIntent::Apply(request) => {
            if !context.caller().is_owner() {
                sqlite_access::require(connection, context, project, ProjectAction::Edit)?;
                access::restricted_edit(request, &|id| {
                    Ok(sqlite_uploads::media(connection, project, id)?.map(|r| r.asset))
                })?;
            }
            ProjectAction::Edit
        }
        EditIntent::Restore(_) => ProjectAction::Restore,
    };
    sqlite_access::require(connection, context, project, action)
}

impl ProjectRead for SqliteConnection {
    fn authorize_edit(&self, project_id: &str, intent: EditIntent<'_>) -> Result<(), EditorError> {
        authorize_edit(self, &self.1, project_id, intent)
    }
    fn head(&self, project_id: &str) -> Result<ProjectSnapshot, EditorError> {
        sqlite_access::require(self, &self.1, project_id, ProjectAction::Read)?;
        load_snapshot(self, project_id)
    }
    fn revision(&self, project_id: &str, revision: &str) -> Result<ProjectSnapshot, EditorError> {
        sqlite_access::require(self, &self.1, project_id, ProjectAction::Read)?;
        load_revision(self, project_id, revision)
    }

    fn replay_create(
        &self,
        input: &CreateProject,
        key: &str,
    ) -> Result<Option<ProjectSnapshot>, EditorError> {
        sqlite_access::owner(&self.1)?;
        let identity = request_hash("create_project", &input.id, input)?;
        load_idempotent(
            self,
            "create_project",
            "workspace",
            key,
            &identity,
            "snapshot",
        )?
        .map(|value| decode_json(&value, "stored project creation result"))
        .transpose()
    }
    fn replay_edit(
        &self,
        project_id: &str,
        intent: EditIntent<'_>,
    ) -> Result<Option<TransactionResult>, EditorError> {
        authorize_edit(self, &self.1, project_id, intent)?;
        let identity = intent.identity(project_id)?;
        load_idempotent(
            self,
            intent.family(),
            project_id,
            intent.key(),
            &identity,
            "transaction",
        )?
        .map(|value| decode_json(&value, "stored transaction result"))
        .transpose()
    }
}
impl ProjectStore for SqliteConnection {
    fn create_atomic(
        &mut self,
        input: &CreateProject,
        idempotency_key: &str,
        snapshot: ProjectSnapshot,
    ) -> Result<ProjectSnapshot, EditorError> {
        require_idempotency_key(idempotency_key)?;
        let request_hash = request_hash("create_project", &input.id, &input)?;
        let context = self.1.clone();
        let tx = self
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;

        sqlite_access::owner(&context)?;

        if let Some(snapshot) = load_idempotent(
            &tx,
            "create_project",
            "workspace",
            idempotency_key,
            &request_hash,
            "snapshot",
        )? {
            return decode_json(&snapshot, "stored project creation result");
        }
        if tx
            .query_row(
                "SELECT 1 FROM projects WHERE project_id = ?1",
                [&input.id],
                |_| Ok(()),
            )
            .optional()
            .map_err(storage_error)?
            .is_some()
        {
            return Err(EditorError::new(
                "ALREADY_EXISTS",
                format!("project {} already exists", input.id),
            ));
        }

        let document_json = encode_json(&snapshot.project, "project document")?;
        let revision = &snapshot.revision;
        let result_json = encode_json(&snapshot, "project snapshot")?;
        let event_id = format!("evt_{}", Uuid::new_v4().simple());
        let event_payload = encode_json(
            &json!({ "project_id": input.id, "revision": revision }),
            "project-created event",
        )?;

        tx.execute(
            "INSERT INTO projects(project_id, name, head_revision) VALUES (?1, ?2, ?3)",
            params![input.id, input.name, revision],
        )
        .map_err(constraint_or_storage)?;
        tx.execute(
            "INSERT INTO revisions(revision, project_id, parent_revision, transaction_id, document_json)
             VALUES (?1, ?2, NULL, NULL, ?3)",
            params![revision, input.id, document_json],
        )
        .map_err(storage_error)?;
        tx.execute(
            "INSERT INTO idempotency(operation_family, scope, idempotency_key, request_hash, result_kind, result_json)
             VALUES ('create_project', 'workspace', ?1, ?2, 'snapshot', ?3)",
            params![idempotency_key, request_hash.current_digest(), result_json],
        )
        .map_err(idempotency_insert_error)?;
        tx.execute(
            "INSERT INTO outbox(event_id, project_id, revision, event_type, payload_json)
             VALUES (?1, ?2, ?3, 'project.created', ?4)",
            params![event_id, input.id, revision, event_payload],
        )
        .map_err(storage_error)?;
        sqlite_audit::insert(
            &tx,
            &context,
            "project.created",
            sqlite_audit::Links {
                project_id: Some(&input.id),
                revision: Some(revision),
                ..Default::default()
            },
        )?;
        tx.commit().map_err(storage_error)?;
        Ok(snapshot)
    }

    fn commit_atomic(
        &mut self,
        intent: EditIntent<'_>,
        prepared: PreparedCommit,
    ) -> Result<TransactionResult, EditorError> {
        let project_id = &prepared.snapshot().project.id;
        let request_hash = intent.identity(project_id)?;
        let context = self.1.clone();
        let tx = self
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        authorize_edit(&tx, &context, project_id, intent)?;
        sqlite_edit_control::require_writable(&tx, &context, project_id)?;
        // Authorization precedes receipt reads; idempotency still precedes head checks.
        if let Some(result) = load_idempotent(
            &tx,
            intent.family(),
            project_id,
            intent.key(),
            &request_hash,
            "transaction",
        )? {
            return decode_json(&result, "stored transaction result");
        }
        let head = load_snapshot(&tx, project_id)?;
        if head.revision != intent.expected_revision() {
            return Err(revision_conflict(
                intent.expected_revision(),
                &head.revision,
            ));
        }
        let result = prepared.result().clone();
        let base_revision = &result.base_revision;
        let revision = &result.revision;
        let transaction_id = &result.transaction_id;
        let document_json = encode_json(&prepared.snapshot().project, "planned project document")?;
        let result_json = encode_json(&result, "transaction result")?;
        let request_json = prepared.request_json();
        let event_id = prepared.event_id();
        let event_payload = &result_json;
        tx.execute(
            "INSERT INTO revisions(revision, project_id, parent_revision, transaction_id, document_json)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![revision, project_id, base_revision, transaction_id, document_json],
        )
        .map_err(storage_error)?;
        let updated = tx
            .execute(
                "UPDATE projects SET head_revision = ?1, name = ?2
                 WHERE project_id = ?3 AND head_revision = ?4",
                params![
                    revision,
                    prepared.snapshot().project.name,
                    project_id,
                    base_revision
                ],
            )
            .map_err(storage_error)?;
        if updated != 1 {
            return Err(EditorError::new(
                "REVISION_CONFLICT",
                "project head changed while committing transaction",
            ));
        }
        tx.execute(
            "INSERT INTO editor_transactions(transaction_id, project_id, base_revision, revision, request_json, result_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![transaction_id, project_id, base_revision, revision, request_json, result_json],
        )
        .map_err(storage_error)?;
        tx.execute(
            "INSERT INTO idempotency(operation_family, scope, idempotency_key, request_hash, result_kind, result_json)
             VALUES (?1, ?2, ?3, ?4, 'transaction', ?5)",
            params![intent.family(), project_id, intent.key(), request_hash.current_digest(), result_json],
        )
        .map_err(idempotency_insert_error)?;
        tx.execute(
            "INSERT INTO outbox(event_id, project_id, revision, event_type, payload_json)
             VALUES (?1, ?2, ?3, 'transaction.committed', ?4)",
            params![event_id, project_id, revision, event_payload],
        )
        .map_err(storage_error)?;
        sqlite_audit::insert(
            &tx,
            &context,
            match intent {
                EditIntent::Apply(_) => "project.applied",
                EditIntent::Restore(_) => "project.restored",
            },
            sqlite_audit::Links {
                project_id: Some(project_id),
                revision: Some(revision),
                transaction_id: Some(transaction_id),
                ..Default::default()
            },
        )?;
        tx.commit().map_err(storage_error)?;
        Ok(result)
    }
}

pub(super) struct NativeAssetPinning<'a> {
    root: &'a Path,
    store: Option<AssetStore>,
}
impl<'a> NativeAssetPinning<'a> {
    pub(super) fn new(root: &'a Path) -> Self {
        Self { root, store: None }
    }
}
impl AssetPinning for NativeAssetPinning<'_> {
    fn pin(
        &mut self,
        asset: &opencut_contracts::Asset,
    ) -> Result<opencut_contracts::Asset, EditorError> {
        if self.store.is_none() {
            self.store = Some(AssetStore::open_managed(self.root)?);
        }
        self.store
            .as_ref()
            .expect("initialized asset store")
            .pin_asset(asset)
    }
}

fn load_revision(
    connection: &Connection,
    project_id: &str,
    revision: &str,
) -> Result<ProjectSnapshot, EditorError> {
    let document: Option<String> = connection
        .query_row(
            "SELECT document_json FROM revisions WHERE project_id=?1 AND revision=?2",
            params![project_id, revision],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage_error)?;
    Ok(ProjectSnapshot {
        project: sqlite_project_v3::decode_legacy_project(&document.ok_or_else(|| {
            EditorError::new(
                "NOT_FOUND",
                "Target revision does not belong to this project",
            )
        })?)?,
        revision: revision.into(),
    })
}

impl opencut_editor_application::access::ProjectExecutionRead for EditorApplication {
    fn execution_snapshot(
        &self,
        project_id: &str,
        revision: &str,
    ) -> Result<opencut_editor_application::access::ExecutionSnapshot, EditorError> {
        use opencut_editor_application::access::{ExecutionSnapshot, SourceAccess};
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        sqlite_access::require(&tx, &self.connection.1, project_id, ProjectAction::RunJobs)?;
        let snapshot = load_revision(&tx, project_id, revision)?;
        let sources = if self.connection.1.caller().is_owner() {
            SourceAccess::OwnerPaths
        } else {
            SourceAccess::ProjectContent
        };
        tx.commit().map_err(storage_error)?;
        Ok(ExecutionSnapshot { snapshot, sources })
    }
}
