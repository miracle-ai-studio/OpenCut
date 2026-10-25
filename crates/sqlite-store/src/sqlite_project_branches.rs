//! Canonical refs and merge publication over existing branch/revision tables.
use super::*;
use opencut_contracts::branching::*;
use opencut_editor_application::{
    branching::{self, BranchStore, ComparisonSource},
    scene::{PreparedSceneCommit, SceneCommitIntent},
};

fn require(
    connection: &Connection,
    context: &opencut_editor_application::audit::ExecutionContext,
    project: &str,
    edit: bool,
) -> Result<(), EditorError> {
    sqlite_branching::require_read(connection, context, project)?;
    sqlite_project_library::require_live(connection, project)?;
    if edit {
        sqlite_branching::require_edit(connection, context, project)?;
    }
    Ok(())
}
fn receipt<T: serde::Serialize>(
    connection: &Connection,
    project: &str,
    key: &str,
    families: [&str; 3],
    input: &T,
) -> Result<Option<(String, String)>, EditorError> {
    let mut statement=connection.prepare("SELECT operation_family,request_hash,result_kind,result_json FROM idempotency WHERE scope=?1 AND idempotency_key=?2 AND operation_family IN (?3,?4,?5)").map_err(storage_error)?;
    let rows = statement
        .query_map(
            params![project, key, families[0], families[1], families[2]],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .map_err(storage_error)?;
    let mut result = None;
    let mut any = false;
    for row in rows {
        let (family, hash, kind, raw) = row.map_err(storage_error)?;
        any = true;
        if !request_hash(&family, project, input)?.matches(&hash, None) {
            continue;
        }
        if result.is_some() {
            return Err(EditorError::new(
                "STORAGE_ERROR",
                "Branch request has ambiguous original receipts",
            ));
        }
        result = Some((kind, raw));
    }
    if any && result.is_none() {
        return Err(EditorError::new(
            "IDEMPOTENCY_KEY_REUSED",
            "Branch key belongs to a different original request",
        ));
    }
    Ok(result)
}
fn creation_receipt(
    connection: &Connection,
    input: &BranchCreateRequest,
) -> Result<Option<BranchView>, EditorError> {
    let Some((kind, raw)) = receipt(
        connection,
        &input.project_id,
        &input.idempotency_key,
        [
            "branch_create",
            "project_v3_branch_create",
            "project_branch_create",
        ],
        input,
    )?
    else {
        return Ok(None);
    };
    if kind != "branch" {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Branch creation receipt kind differs",
        ));
    }
    let view: BranchView = decode_json(&raw, "original branch creation receipt")?;
    if view.project_id != input.project_id
        || view.branch_name != input.branch_name
        || view.head_revision != input.from_revision
        || view.created_from_revision != input.from_revision
        || view.is_main
    {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Branch creation receipt differs from its original scope",
        ));
    }
    sqlite_scene::load_source(connection, &input.project_id, Some(&input.from_revision))?;
    Ok(Some(view))
}
pub(super) fn merge_receipt(
    connection: &Connection,
    input: &BranchMergeRequest,
) -> Result<Option<BranchMergeCommit>, EditorError> {
    let Some((kind, raw)) = receipt(
        connection,
        &input.project_id,
        &input.idempotency_key,
        [
            "branch_merge",
            "project_v3_branch_merge",
            "project_branch_merge",
        ],
        input,
    )?
    else {
        return Ok(None);
    };
    if kind != "branch_merge" {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Branch merge receipt kind differs",
        ));
    }
    let value: BranchMergeCommit = decode_json(&raw, "original branch merge receipt")?;
    let base = sqlite_branching::common_ancestor(
        connection,
        &input.project_id,
        &input.expected_source_revision,
        &input.expected_target_revision,
    )?;
    let transaction = &value.transaction;
    let exact:bool=connection.query_row("SELECT EXISTS(SELECT 1 FROM editor_transactions t JOIN revisions r ON r.project_id=t.project_id AND r.revision=t.revision JOIN project_merge_parents m ON m.project_id=t.project_id AND m.revision=t.revision WHERE t.project_id=?1 AND t.transaction_id=?2 AND t.base_revision=?3 AND t.revision=?4 AND m.source_revision=?5)",params![input.project_id,transaction.transaction_id,input.expected_target_revision,transaction.revision,input.expected_source_revision],|row|row.get(0)).map_err(storage_error)?;
    if !exact
        || value.source_branch != input.source_branch
        || value.target_branch != input.target_branch
        || value.source_revision != input.expected_source_revision
        || value.base_revision != base
        || transaction.base_revision != input.expected_target_revision
    {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Branch merge receipt differs from its immutable scoped publication",
        ));
    }
    Ok(Some(value))
}
/// Check both heads and the exact typed causal journal under publication lock.
/// Caller retains the shared CAS guard and performs the ordinary editor commit.
pub(super) fn publication(
    connection: &Connection,
    input: &BranchMergeRequest,
    prepared: &PreparedSceneCommit,
) -> Result<BranchMergeCommit, EditorError> {
    let source = sqlite_branching::branch(connection, &input.project_id, &input.source_branch)?;
    if source.head_revision != input.expected_source_revision {
        return Err(revision_conflict(
            &input.expected_source_revision,
            &source.head_revision,
        ));
    }
    let base = sqlite_branching::common_ancestor(
        connection,
        &input.project_id,
        &input.expected_source_revision,
        &input.expected_target_revision,
    )?;
    let journal: BranchJournal =
        decode_json(prepared.journal_json(), "prepared branch merge journal")?;
    let BranchJournal::BranchMerge {
        request,
        base_revision,
        changes,
    } = journal;
    if request != *input || base_revision != base || changes.is_empty() {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Prepared branch journal differs from the fixed merge",
        ));
    }
    Ok(BranchMergeCommit {
        source_branch: input.source_branch.clone(),
        target_branch: input.target_branch.clone(),
        base_revision: base,
        source_revision: input.expected_source_revision.clone(),
        transaction: prepared.result().clone(),
    })
}
impl BranchStore for EditorApplication {
    fn read_branch_refs(&self, project: &str) -> Result<Vec<BranchView>, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        require(&tx, &self.connection.1, project, false)?;
        let mut views = vec![sqlite_branching::branch(&tx, project, "main")?];
        let mut statement = tx
            .prepare(
                "SELECT branch_name FROM project_branches WHERE project_id=?1 ORDER BY branch_name",
            )
            .map_err(storage_error)?;
        let rows = statement
            .query_map([project], |row| row.get::<_, String>(0))
            .map_err(storage_error)?;
        for row in rows {
            views.push(sqlite_branching::branch(
                &tx,
                project,
                &row.map_err(storage_error)?,
            )?);
        }
        drop(statement);
        tx.commit().map_err(storage_error)?;
        Ok(views)
    }
    fn create_branch_ref(
        &mut self,
        input: &BranchCreateRequest,
    ) -> Result<BranchView, EditorError> {
        let context = self.connection.1.clone();
        let identity = request_hash("project_branch_create", &input.project_id, input)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        require(&tx, &context, &input.project_id, true)?;
        sqlite_edit_control::require_writable(&tx, &context, &input.project_id)?;
        if let Some(value) = creation_receipt(&tx, input)? {
            return Ok(value);
        }
        // Existing exact historical receipts were handled above. New publication
        // uses the same cross-operation project-key fence as ordinary edits.
        let _ = load_idempotent(
            &tx,
            "project_branch_create",
            &input.project_id,
            &input.idempotency_key,
            &identity,
            "branch",
        )?;
        let current = sqlite_branching::branch(&tx, &input.project_id, "main")?;
        if current.head_revision != input.expected_revision {
            return Err(revision_conflict(
                &input.expected_revision,
                &current.head_revision,
            ));
        }
        sqlite_scene::load_source(&tx, &input.project_id, Some(&input.from_revision))?;
        let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM project_branches WHERE project_id=?1 AND branch_name=?2)",params![input.project_id,input.branch_name],|row|row.get(0)).map_err(storage_error)?;
        if exists {
            return Err(EditorError::new("ALREADY_EXISTS", "Branch already exists"));
        }
        let view = BranchView {
            project_id: input.project_id.clone(),
            branch_name: input.branch_name.clone(),
            head_revision: input.from_revision.clone(),
            created_from_revision: input.from_revision.clone(),
            is_main: false,
        };
        tx.execute("INSERT INTO project_branches(project_id,branch_name,head_revision,created_from_revision) VALUES (?1,?2,?3,?3)",params![input.project_id,input.branch_name,input.from_revision]).map_err(constraint_or_storage)?;
        let raw = encode_json(&view, "branch creation receipt")?;
        tx.execute("INSERT INTO idempotency(operation_family,scope,idempotency_key,request_hash,result_kind,result_json) VALUES ('project_branch_create',?1,?2,?3,'branch',?4)",params![input.project_id,input.idempotency_key,identity.current_digest(),raw]).map_err(idempotency_insert_error)?;
        tx.execute("INSERT INTO outbox(event_id,project_id,revision,event_type,payload_json) VALUES (?1,?2,?3,'branch.created',?4)",params![format!("evt_{}",Uuid::new_v4().simple()),input.project_id,input.from_revision,raw]).map_err(storage_error)?;
        sqlite_audit::insert(
            &tx,
            &context,
            "project.branch_created",
            sqlite_audit::Links {
                project_id: Some(&input.project_id),
                revision: Some(&input.from_revision),
                ..Default::default()
            },
        )?;
        tx.commit().map_err(storage_error)?;
        Ok(view)
    }
    fn comparison_source(
        &self,
        input: &BranchCompareRequest,
    ) -> Result<ComparisonSource, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        require(&tx, &self.connection.1, &input.project_id, true)?;
        sqlite_scene::resources::authorize_branch_versions(
            &tx,
            &self.connection.1,
            &input.project_id,
            &input.base_asset_versions,
        )?;
        let source = sqlite_branching::branch(&tx, &input.project_id, &input.source_branch)?;
        let target = sqlite_branching::branch(&tx, &input.project_id, &input.target_branch)?;
        if source.head_revision != input.expected_source_revision {
            return Err(revision_conflict(
                &input.expected_source_revision,
                &source.head_revision,
            ));
        }
        if target.head_revision != input.expected_target_revision {
            return Err(revision_conflict(
                &input.expected_target_revision,
                &target.head_revision,
            ));
        }
        let base = sqlite_branching::common_ancestor(
            &tx,
            &input.project_id,
            &source.head_revision,
            &target.head_revision,
        )?;
        let result = ComparisonSource {
            base: sqlite_scene::load_source(&tx, &input.project_id, Some(&base))?,
            source: sqlite_scene::load_source(&tx, &input.project_id, Some(&source.head_revision))?,
            target: sqlite_scene::load_source(&tx, &input.project_id, Some(&target.head_revision))?,
        };
        tx.commit().map_err(storage_error)?;
        Ok(result)
    }
    fn replay_branch_merge(
        &self,
        input: &BranchMergeRequest,
    ) -> Result<Option<BranchMergeCommit>, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        sqlite_scene::authorize(
            &tx,
            &self.connection.1,
            SceneCommitIntent::BranchMerge(input),
        )?;
        let result = merge_receipt(&tx, input)?;
        // Enforce the shared key fence even when no matching receipt exists.
        if result.is_none() {
            let _ = load_idempotent(
                &tx,
                "project_branch_merge",
                &input.project_id,
                &input.idempotency_key,
                &request_hash("project_branch_merge", &input.project_id, input)?,
                "branch_merge",
            )?;
        }
        tx.commit().map_err(storage_error)?;
        Ok(result)
    }
}
impl EditorApplication {
    pub fn list_branches(&self, input: &BranchListRequest) -> Result<Vec<BranchView>, EditorError> {
        branching::list(self, input)
    }
    pub fn create_branch(
        &mut self,
        input: &BranchCreateRequest,
    ) -> Result<BranchView, EditorError> {
        branching::create(self, input)
    }
    pub fn compare_branches(
        &self,
        input: &BranchCompareRequest,
    ) -> Result<BranchComparePlan, EditorError> {
        branching::compare(self, input)
    }
    pub fn merge_branches(
        &mut self,
        input: &BranchMergeRequest,
    ) -> Result<BranchMergeCommit, EditorError> {
        branching::merge(self, input)
    }
}
