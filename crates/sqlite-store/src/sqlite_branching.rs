//! Native branch references and three-way merge storage.
pub(super) const MIGRATION: &str = "
CREATE TABLE project_branches (
 project_id TEXT NOT NULL REFERENCES projects(project_id),
 branch_name TEXT NOT NULL CHECK(branch_name <> 'main'),
 head_revision TEXT NOT NULL REFERENCES revisions(revision),
 created_from_revision TEXT NOT NULL REFERENCES revisions(revision),
 created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
 PRIMARY KEY(project_id,branch_name)
);
CREATE INDEX project_branches_head_idx ON project_branches(head_revision);
CREATE TABLE project_merge_parents (
 revision TEXT PRIMARY KEY REFERENCES revisions(revision),
 project_id TEXT NOT NULL REFERENCES projects(project_id),
 source_revision TEXT NOT NULL REFERENCES revisions(revision)
);
CREATE INDEX project_merge_source_idx ON project_merge_parents(project_id,source_revision);
UPDATE metadata SET value='20' WHERE key='schema_version' AND value='19';
";

use super::*;
use opencut_api_contracts::branching::{
    BranchComparePlan, BranchCompareRequest, BranchCreateRequest, BranchEditRequest,
    BranchListRequest, BranchMergeCommit, BranchMergeRequest, BranchView,
};
use opencut_contracts::{Operation, ProjectDocument};
use opencut_editor_application::{access, projects::AssetPinning};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};

pub(super) fn branch_name(value: &str, main_allowed: bool) -> Result<(), EditorError> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        || (!main_allowed && value == "main")
    {
        return Err(EditorError::new("INVALID_ARGUMENT", "Invalid branch name"));
    }
    Ok(())
}

fn require_read(
    connection: &Connection,
    context: &opencut_editor_application::audit::ExecutionContext,
    project: &str,
) -> Result<(), EditorError> {
    sqlite_access::require(connection, context, project, ProjectAction::Read)
}
fn require_edit(
    connection: &Connection,
    context: &opencut_editor_application::audit::ExecutionContext,
    project: &str,
) -> Result<(), EditorError> {
    require_read(connection, context, project)?;
    sqlite_access::require(connection, context, project, ProjectAction::Edit)
}

pub(super) fn root_revision(connection: &Connection, project: &str) -> Result<String, EditorError> {
    connection.query_row("SELECT revision FROM revisions WHERE project_id=?1 AND parent_revision IS NULL ORDER BY rowid LIMIT 1",[project],|row|row.get(0))
        .optional().map_err(storage_error)?.ok_or_else(||EditorError::new("NOT_FOUND","Project has no root revision"))
}

pub(super) fn branch(
    connection: &Connection,
    project: &str,
    name: &str,
) -> Result<BranchView, EditorError> {
    branch_name(name, true)?;
    if name == "main" {
        let head = load_snapshot(connection, project)?;
        return Ok(BranchView {
            project_id: project.into(),
            branch_name: name.into(),
            head_revision: head.revision,
            created_from_revision: root_revision(connection, project)?,
            is_main: true,
        });
    }
    let row: Option<(String,String)> = connection.query_row(
        "SELECT b.head_revision,b.created_from_revision FROM project_branches b JOIN revisions r ON r.revision=b.head_revision AND r.project_id=b.project_id WHERE b.project_id=?1 AND b.branch_name=?2",
        params![project,name],|row|Ok((row.get(0)?,row.get(1)?))
    ).optional().map_err(storage_error)?;
    let (head_revision, created_from_revision) =
        row.ok_or_else(|| EditorError::new("NOT_FOUND", "Branch does not exist in this project"))?;
    Ok(BranchView {
        project_id: project.into(),
        branch_name: name.into(),
        head_revision,
        created_from_revision,
        is_main: false,
    })
}

fn document(
    connection: &Connection,
    project: &str,
    revision: &str,
) -> Result<ProjectDocument, EditorError> {
    let raw: Option<String> = connection
        .query_row(
            "SELECT document_json FROM revisions WHERE project_id=?1 AND revision=?2",
            params![project, revision],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage_error)?;
    sqlite_project_v3::decode_legacy_project(
        &raw.ok_or_else(|| {
            EditorError::new("NOT_FOUND", "Revision does not belong to this project")
        })?,
    )
}

pub(super) fn parents(
    connection: &Connection,
    project: &str,
    revision: &str,
) -> Result<Vec<String>, EditorError> {
    let primary: Option<Option<String>> = connection
        .query_row(
            "SELECT parent_revision FROM revisions WHERE project_id=?1 AND revision=?2",
            params![project, revision],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage_error)?;
    let primary = primary
        .ok_or_else(|| EditorError::new("NOT_FOUND", "Revision does not belong to this project"))?;
    let secondary: Option<String> = connection
        .query_row(
            "SELECT source_revision FROM project_merge_parents WHERE project_id=?1 AND revision=?2",
            params![project, revision],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage_error)?;
    Ok(primary.into_iter().chain(secondary).collect())
}

fn distances(
    connection: &Connection,
    project: &str,
    head: &str,
) -> Result<HashMap<String, usize>, EditorError> {
    let mut seen = HashMap::new();
    let mut queue = VecDeque::from([(head.to_owned(), 0usize)]);
    while let Some((revision, distance)) = queue.pop_front() {
        if seen.contains_key(&revision) {
            continue;
        }
        if seen.len() >= 100_000 {
            return Err(EditorError::new(
                "STORAGE_ERROR",
                "Branch ancestry exceeds traversal limit",
            ));
        }
        seen.insert(revision.clone(), distance);
        for parent in parents(connection, project, &revision)? {
            queue.push_back((parent, distance + 1));
        }
    }
    Ok(seen)
}

pub(super) fn common_ancestor(
    connection: &Connection,
    project: &str,
    source: &str,
    target: &str,
) -> Result<String, EditorError> {
    let source_ancestors = distances(connection, project, source)?;
    let target_ancestors = distances(connection, project, target)?;
    let common: std::collections::HashSet<_> = source_ancestors
        .keys()
        .filter(|revision| target_ancestors.contains_key(*revision))
        .cloned()
        .collect();
    if common.is_empty() {
        return Err(EditorError::new(
            "CAUSAL_CONFLICT",
            "Branches have no common project ancestor",
        ));
    }
    // A latest common ancestor is not necessarily the one with the shortest
    // summed path in a merge DAG. Walk newest revisions first and discard all
    // older common ancestors reachable from each candidate.
    let mut statement = connection
        .prepare("SELECT revision FROM revisions WHERE project_id=?1 ORDER BY rowid DESC")
        .map_err(storage_error)?;
    let rows = statement
        .query_map([project], |row| row.get::<_, String>(0))
        .map_err(storage_error)?;
    let mut newest = Vec::new();
    let mut excluded = std::collections::HashSet::new();
    for row in rows {
        let revision = row.map_err(storage_error)?;
        if !common.contains(&revision) || excluded.contains(&revision) {
            continue;
        }
        newest.push(revision.clone());
        let mut pending = VecDeque::from([revision]);
        let mut visited = std::collections::HashSet::new();
        while let Some(node) = pending.pop_front() {
            if !visited.insert(node.clone()) {
                continue;
            }
            if visited.len() > 100_000 {
                return Err(EditorError::new(
                    "STORAGE_ERROR",
                    "Merge ancestry exceeds traversal limit",
                ));
            }
            for parent in parents(connection, project, &node)? {
                if common.contains(&parent) {
                    excluded.insert(parent.clone());
                }
                pending.push_back(parent);
            }
        }
    }
    if newest.len() != 1 {
        return Err(EditorError::new(
            "CAUSAL_CONFLICT",
            "Branches have multiple incomparable merge bases",
        ));
    }
    Ok(newest.remove(0))
}

fn compare_on(
    connection: &Connection,
    context: &opencut_editor_application::audit::ExecutionContext,
    input: &BranchCompareRequest,
) -> Result<BranchComparePlan, EditorError> {
    require_read(connection, context, &input.project_id)?;
    if input.source_branch == input.target_branch {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Source and target branches must differ",
        ));
    }
    let source = branch(connection, &input.project_id, &input.source_branch)?;
    let target = branch(connection, &input.project_id, &input.target_branch)?;
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
    let base = common_ancestor(
        connection,
        &input.project_id,
        &source.head_revision,
        &target.head_revision,
    )?;
    Ok(opencut_editor_application::branching::compare(
        input,
        &base,
        &document(connection, &input.project_id, &base)?,
        &document(connection, &input.project_id, &source.head_revision)?,
        &document(connection, &input.project_id, &target.head_revision)?,
    ))
}

fn revision_id(
    parent: &str,
    transaction_id: &str,
    project: &ProjectDocument,
) -> Result<String, EditorError> {
    let material = encode_json(
        &json!({"parent":parent,"transaction_id":transaction_id,"document":project}),
        "branch revision material",
    )?;
    let digest = Sha256::digest(material.as_bytes());
    Ok(format!(
        "rev_{}",
        digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    ))
}

fn compare_input(input: &BranchMergeRequest) -> BranchCompareRequest {
    BranchCompareRequest {
        project_id: input.project_id.clone(),
        source_branch: input.source_branch.clone(),
        target_branch: input.target_branch.clone(),
        expected_source_revision: input.expected_source_revision.clone(),
        expected_target_revision: input.expected_target_revision.clone(),
    }
}

impl EditorApplication {
    pub fn list_branches(&self, input: &BranchListRequest) -> Result<Vec<BranchView>, EditorError> {
        require_read(&self.connection, &self.connection.1, &input.project_id)?;
        let mut views = vec![branch(&self.connection, &input.project_id, "main")?];
        let mut statement = self
            .connection
            .prepare(
                "SELECT branch_name FROM project_branches WHERE project_id=?1 ORDER BY branch_name",
            )
            .map_err(storage_error)?;
        let names = statement
            .query_map([&input.project_id], |row| row.get::<_, String>(0))
            .map_err(storage_error)?;
        for name in names {
            views.push(branch(
                &self.connection,
                &input.project_id,
                &name.map_err(storage_error)?,
            )?);
        }
        Ok(views)
    }

    pub fn create_branch(
        &mut self,
        input: &BranchCreateRequest,
    ) -> Result<BranchView, EditorError> {
        branch_name(&input.branch_name, false)?;
        require_idempotency_key(&input.idempotency_key)?;
        let identity = request_hash("branch_create", &input.project_id, input)?;
        let context = self.connection.1.clone();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        require_edit(&tx, &context, &input.project_id)?;
        sqlite_edit_control::require_writable(&tx, &context, &input.project_id)?;
        if let Some(value) = load_idempotent(
            &tx,
            "branch_create",
            &input.project_id,
            &input.idempotency_key,
            &identity,
            "branch",
        )? {
            return decode_json(&value, "branch creation receipt");
        }
        let current = load_snapshot(&tx, &input.project_id)?;
        if current.revision != input.expected_revision {
            return Err(revision_conflict(
                &input.expected_revision,
                &current.revision,
            ));
        }
        let _ = document(&tx, &input.project_id, &input.from_revision)?;
        if tx
            .query_row(
                "SELECT 1 FROM project_branches WHERE project_id=?1 AND branch_name=?2",
                params![input.project_id, input.branch_name],
                |_| Ok(()),
            )
            .optional()
            .map_err(storage_error)?
            .is_some()
        {
            return Err(EditorError::new("ALREADY_EXISTS", "Branch already exists"));
        }
        let view = BranchView {
            project_id: input.project_id.clone(),
            branch_name: input.branch_name.clone(),
            head_revision: input.from_revision.clone(),
            created_from_revision: input.from_revision.clone(),
            is_main: false,
        };
        tx.execute("INSERT INTO project_branches(project_id,branch_name,head_revision,created_from_revision) VALUES (?1,?2,?3,?4)",
            params![input.project_id,input.branch_name,input.from_revision,input.from_revision]).map_err(constraint_or_storage)?;
        tx.execute("INSERT INTO idempotency(operation_family,scope,idempotency_key,request_hash,result_kind,result_json) VALUES ('branch_create',?1,?2,?3,'branch',?4)",
            params![input.project_id,input.idempotency_key,identity.current_digest(),encode_json(&view,"branch receipt")?]).map_err(idempotency_insert_error)?;
        tx.execute("INSERT INTO outbox(event_id,project_id,revision,event_type,payload_json) VALUES (?1,?2,?3,'branch.created',?4)",
            params![format!("evt_{}",Uuid::new_v4().simple()),input.project_id,input.from_revision,encode_json(&view,"branch event")?]).map_err(storage_error)?;
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

    pub fn compare_branches(
        &self,
        input: &BranchCompareRequest,
    ) -> Result<BranchComparePlan, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let result = compare_on(&tx, &self.connection.1, input)?;
        tx.commit().map_err(storage_error)?;
        Ok(result)
    }

    pub fn apply_branch_edit(
        &mut self,
        input: &BranchEditRequest,
    ) -> Result<TransactionResult, EditorError> {
        branch_name(&input.branch_name, false)?;
        require_idempotency_key(&input.request.idempotency_key)?;
        let identity = request_hash("branch_apply", &input.project_id, input)?;
        let context = self.connection.1.clone();
        require_edit(&self.connection, &context, &input.project_id)?;
        if !context.caller().is_owner() {
            access::restricted_edit(&input.request, &|id| {
                Ok(
                    sqlite_uploads::media(&self.connection, &input.project_id, id)?
                        .map(|r| r.asset),
                )
            })?;
        }
        if let Some(value) = load_idempotent(
            &self.connection,
            "branch_apply",
            &input.project_id,
            &input.request.idempotency_key,
            &identity,
            "transaction",
        )? {
            return decode_json(&value, "branch edit receipt");
        }
        // Native CAS pinning occurs before the SQLite write lock, as with main edits.
        let mut prepared = input.request.clone();
        let mut pinning = sqlite_projects::NativeAssetPinning::new(&self.asset_store_root);
        for operation in &mut prepared.operations {
            if let Operation::BindAsset { asset } | Operation::ReplaceAsset { asset } = operation {
                *asset = pinning.pin(asset)?;
            }
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        require_edit(&tx, &context, &input.project_id)?;
        sqlite_edit_control::require_writable(&tx, &context, &input.project_id)?;
        if !context.caller().is_owner() {
            access::restricted_edit(&input.request, &|id| {
                Ok(sqlite_uploads::media(&tx, &input.project_id, id)?.map(|r| r.asset))
            })?;
        }
        if let Some(value) = load_idempotent(
            &tx,
            "branch_apply",
            &input.project_id,
            &input.request.idempotency_key,
            &identity,
            "transaction",
        )? {
            return decode_json(&value, "branch edit receipt");
        }
        let current = branch(&tx, &input.project_id, &input.branch_name)?;
        if current.head_revision != input.request.expected_revision {
            return Err(revision_conflict(
                &input.request.expected_revision,
                &current.head_revision,
            ));
        }
        let snapshot = ProjectSnapshot {
            revision: current.head_revision.clone(),
            project: document(&tx, &input.project_id, &current.head_revision)?,
        };
        let planned = opencut_editor_core::plan_transaction(&snapshot, &prepared)?;
        let transaction_id = format!("txn_{}", Uuid::new_v4().simple());
        let revision = revision_id(
            &current.head_revision,
            &transaction_id,
            &planned.proposed_project,
        )?;
        let result = TransactionResult {
            transaction_id: transaction_id.clone(),
            base_revision: current.head_revision.clone(),
            revision: revision.clone(),
            created_ids: planned.created_ids,
            changed_ids: planned.changed_ids,
            deleted_ids: planned.deleted_ids,
        };
        tx.execute("INSERT INTO revisions(revision,project_id,parent_revision,transaction_id,document_json) VALUES (?1,?2,?3,?4,?5)",
            params![revision,input.project_id,current.head_revision,transaction_id,encode_json(&planned.proposed_project,"branch edit project")?]).map_err(storage_error)?;
        let updated = tx.execute("UPDATE project_branches SET head_revision=?1 WHERE project_id=?2 AND branch_name=?3 AND head_revision=?4",
            params![revision,input.project_id,input.branch_name,current.head_revision]).map_err(storage_error)?;
        if updated != 1 {
            return Err(EditorError::new(
                "REVISION_CONFLICT",
                "Branch head changed during edit",
            ));
        }
        tx.execute("INSERT INTO editor_transactions(transaction_id,project_id,base_revision,revision,request_json,result_json) VALUES (?1,?2,?3,?4,?5,?6)",
            params![transaction_id,input.project_id,current.head_revision,revision,encode_json(&json!({"kind":"branch_apply","branch_name":input.branch_name,"request":prepared}),"branch edit request")?,encode_json(&result,"branch edit result")?]).map_err(storage_error)?;
        tx.execute("INSERT INTO idempotency(operation_family,scope,idempotency_key,request_hash,result_kind,result_json) VALUES ('branch_apply',?1,?2,?3,'transaction',?4)",
            params![input.project_id,input.request.idempotency_key,identity.current_digest(),encode_json(&result,"branch edit receipt")?]).map_err(idempotency_insert_error)?;
        tx.execute("INSERT INTO outbox(event_id,project_id,revision,event_type,payload_json) VALUES (?1,?2,?3,'branch.transaction.committed',?4)",
            params![format!("evt_{}",Uuid::new_v4().simple()),input.project_id,revision,encode_json(&json!({"branch_name":input.branch_name,"transaction":result}),"branch edit event")?]).map_err(storage_error)?;
        sqlite_audit::insert(
            &tx,
            &context,
            "project.branch_applied",
            sqlite_audit::Links {
                project_id: Some(&input.project_id),
                revision: Some(&revision),
                transaction_id: Some(&transaction_id),
                ..Default::default()
            },
        )?;
        tx.commit().map_err(storage_error)?;
        Ok(result)
    }

    pub fn merge_branches(
        &mut self,
        input: &BranchMergeRequest,
    ) -> Result<BranchMergeCommit, EditorError> {
        require_idempotency_key(&input.idempotency_key)?;
        let identity = request_hash("branch_merge", &input.project_id, input)?;
        let context = self.connection.1.clone();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        require_edit(&tx, &context, &input.project_id)?;
        sqlite_edit_control::require_writable(&tx, &context, &input.project_id)?;
        if let Some(value) = load_idempotent(
            &tx,
            "branch_merge",
            &input.project_id,
            &input.idempotency_key,
            &identity,
            "branch_merge",
        )? {
            return decode_json(&value, "branch merge receipt");
        }
        let plan = compare_on(&tx, &context, &compare_input(input))?;
        if !plan.conflicts.is_empty() {
            return Err(EditorError {
                code: "BRANCH_MERGE_CONFLICT".into(),
                message: "Branches have conflicting changes".into(),
                details: json!({"conflicts":plan.conflicts}),
            });
        }
        if plan.changes.is_empty() {
            return Err(EditorError::new(
                "NO_EFFECT",
                "Source branch has no unmerged changes",
            ));
        }
        let project = plan.proposed_project.ok_or_else(|| {
            EditorError::new("BRANCH_MERGE_CONFLICT", "Merge has no valid project result")
        })?;
        let target_document = document(&tx, &input.project_id, &plan.target_revision)?;
        sqlite_collaboration::restricted_asset_bindings(
            &tx,
            &context,
            &input.project_id,
            &target_document,
            &project,
        )?;
        let transaction_id = format!("txn_{}", Uuid::new_v4().simple());
        let revision = revision_id(&plan.target_revision, &transaction_id, &project)?;
        let (created_ids, changed_ids, deleted_ids) =
            sqlite_collaboration::result_ids(&input.project_id, &plan.changes);
        let result = TransactionResult {
            transaction_id: transaction_id.clone(),
            base_revision: plan.target_revision.clone(),
            revision: revision.clone(),
            created_ids,
            changed_ids,
            deleted_ids,
        };
        let receipt = BranchMergeCommit {
            source_branch: input.source_branch.clone(),
            target_branch: input.target_branch.clone(),
            base_revision: plan.base_revision.clone(),
            source_revision: plan.source_revision.clone(),
            transaction: result.clone(),
        };
        tx.execute("INSERT INTO revisions(revision,project_id,parent_revision,transaction_id,document_json) VALUES (?1,?2,?3,?4,?5)",
            params![revision,input.project_id,plan.target_revision,transaction_id,encode_json(&project,"merged project")?]).map_err(storage_error)?;
        let updated = if input.target_branch == "main" {
            tx.execute("UPDATE projects SET head_revision=?1,name=?2 WHERE project_id=?3 AND head_revision=?4",
                params![revision,project.name,input.project_id,plan.target_revision]).map_err(storage_error)?
        } else {
            tx.execute("UPDATE project_branches SET head_revision=?1 WHERE project_id=?2 AND branch_name=?3 AND head_revision=?4",
                params![revision,input.project_id,input.target_branch,plan.target_revision]).map_err(storage_error)?
        };
        if updated != 1 {
            return Err(EditorError::new(
                "REVISION_CONFLICT",
                "Target branch head changed during merge",
            ));
        }
        tx.execute("INSERT INTO project_merge_parents(revision,project_id,source_revision) VALUES (?1,?2,?3)",
            params![revision,input.project_id,plan.source_revision]).map_err(storage_error)?;
        tx.execute("INSERT INTO editor_transactions(transaction_id,project_id,base_revision,revision,request_json,result_json) VALUES (?1,?2,?3,?4,?5,?6)",
            params![transaction_id,input.project_id,plan.target_revision,revision,
                encode_json(&json!({"kind":"branch_merge","source_branch":input.source_branch,
                    "target_branch":input.target_branch,"base_revision":plan.base_revision,
                    "source_revision":plan.source_revision,"expected_target_revision":plan.target_revision,
                    "idempotency_key":input.idempotency_key,"changes":plan.changes}),"branch merge journal")?,
                encode_json(&result,"branch merge transaction")?]).map_err(storage_error)?;
        tx.execute("INSERT INTO idempotency(operation_family,scope,idempotency_key,request_hash,result_kind,result_json) VALUES ('branch_merge',?1,?2,?3,'branch_merge',?4)",
            params![input.project_id,input.idempotency_key,identity.current_digest(),encode_json(&receipt,"branch merge receipt")?]).map_err(idempotency_insert_error)?;
        let event_type = if input.target_branch == "main" {
            "transaction.committed"
        } else {
            "branch.merged"
        };
        let payload = if input.target_branch == "main" {
            encode_json(&result, "main merge event")?
        } else {
            encode_json(&receipt, "branch merge event")?
        };
        tx.execute("INSERT INTO outbox(event_id,project_id,revision,event_type,payload_json) VALUES (?1,?2,?3,?4,?5)",
            params![format!("evt_{}",Uuid::new_v4().simple()),input.project_id,revision,event_type,payload]).map_err(storage_error)?;
        sqlite_audit::insert(
            &tx,
            &context,
            "project.branch_merged",
            sqlite_audit::Links {
                project_id: Some(&input.project_id),
                revision: Some(&revision),
                transaction_id: Some(&transaction_id),
                ..Default::default()
            },
        )?;
        tx.commit().map_err(storage_error)?;
        Ok(receipt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencut_contracts::{CreateProject, FrameRate, TransactionRequest};
    use opencut_editor_application::access::{Caller, PrincipalPolicy, ProjectGrant};

    fn edit(
        revision: &str,
        key: &str,
        name: Option<&str>,
        width: Option<u32>,
        height: Option<u32>,
    ) -> TransactionRequest {
        serde_json::from_value(json!({"expected_revision":revision,"idempotency_key":key,"label":key,
            "operations":[{"type":"sequence.update_settings","sequence_id":"main","name":name,"width":width,"height":height,"fps":null}]})).unwrap()
    }
    fn project(app: &mut EditorApplication, id: &str) -> ProjectSnapshot {
        app.create_project(
            CreateProject {
                id: id.into(),
                name: "Project".into(),
                sequence_id: "main".into(),
                width: 640,
                height: 360,
                fps: FrameRate {
                    numerator: 25,
                    denominator: 1,
                },
            },
            &format!("create-{id}"),
        )
        .unwrap()
    }
    fn create(app: &mut EditorApplication, root: &str, name: &str) -> BranchView {
        app.create_branch(&BranchCreateRequest {
            project_id: "p".into(),
            branch_name: name.into(),
            from_revision: root.into(),
            expected_revision: root.into(),
            idempotency_key: format!("create-{name}"),
        })
        .unwrap()
    }

    #[test]
    fn branch_edit_and_merge_preserve_main_work_and_record_second_parent() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("workspace.sqlite");
        let mut app = EditorApplication::open(&database).unwrap();
        let initial = project(&mut app, "p");
        let branch = create(&mut app, &initial.revision, "agent");
        assert_eq!(
            app.create_branch(&BranchCreateRequest {
                project_id: "p".into(),
                branch_name: "agent".into(),
                from_revision: initial.revision.clone(),
                expected_revision: initial.revision.clone(),
                idempotency_key: "create-agent".into()
            })
            .unwrap(),
            branch
        );
        let branch_edit = BranchEditRequest {
            project_id: "p".into(),
            branch_name: "agent".into(),
            request: edit(&initial.revision, "agent-width", None, Some(800), None),
        };
        let agent = app.apply_branch_edit(&branch_edit).unwrap();
        assert_eq!(app.apply_branch_edit(&branch_edit).unwrap(), agent);
        assert_eq!(
            app.snapshot("p").unwrap().revision,
            initial.revision,
            "branch edit must not move main"
        );
        let human = app
            .apply(
                "p",
                &edit(
                    &initial.revision,
                    "human-name",
                    Some("Human main"),
                    None,
                    None,
                ),
            )
            .unwrap();
        let compare = BranchCompareRequest {
            project_id: "p".into(),
            source_branch: "agent".into(),
            target_branch: "main".into(),
            expected_source_revision: agent.revision.clone(),
            expected_target_revision: human.revision.clone(),
        };
        let plan = app.compare_branches(&compare).unwrap();
        assert_eq!(plan.base_revision, initial.revision);
        assert!(plan.conflicts.is_empty(), "{:?}", plan.conflicts);
        let proposed = plan.proposed_project.unwrap();
        assert_eq!(proposed.sequences[0].name, "Human main");
        assert_eq!(proposed.sequences[0].width, 800);
        let merge = BranchMergeRequest {
            project_id: "p".into(),
            source_branch: "agent".into(),
            target_branch: "main".into(),
            expected_source_revision: agent.revision.clone(),
            expected_target_revision: human.revision.clone(),
            idempotency_key: "merge-agent".into(),
        };
        let merged = app.merge_branches(&merge).unwrap();
        assert_eq!(app.merge_branches(&merge).unwrap(), merged);
        assert_eq!(
            app.snapshot("p").unwrap().revision,
            merged.transaction.revision
        );
        assert_eq!(
            app.list_branches(&BranchListRequest {
                project_id: "p".into()
            })
            .unwrap()[1]
                .head_revision,
            agent.revision
        );
        let parent: String = app
            .connection
            .query_row(
                "SELECT source_revision FROM project_merge_parents WHERE revision=?1",
                [&merged.transaction.revision],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(parent, agent.revision);
        drop(app);
        let mut app = EditorApplication::open(&database).unwrap();
        let after = app
            .compare_branches(&BranchCompareRequest {
                expected_target_revision: merged.transaction.revision.clone(),
                ..compare
            })
            .unwrap();
        assert_eq!(
            after.base_revision, agent.revision,
            "merge source is a causal parent"
        );
        assert!(after.changes.is_empty());
        assert_eq!(
            app.snapshot("p").unwrap().project.sequences[0].name,
            "Human main"
        );
        let next = app
            .apply_branch_edit(&BranchEditRequest {
                project_id: "p".into(),
                branch_name: "agent".into(),
                request: edit(&agent.revision, "agent-height", None, None, Some(400)),
            })
            .unwrap();
        let next_compare = BranchCompareRequest {
            project_id: "p".into(),
            source_branch: "agent".into(),
            target_branch: "main".into(),
            expected_source_revision: next.revision.clone(),
            expected_target_revision: merged.transaction.revision.clone(),
        };
        let plan = app.compare_branches(&next_compare).unwrap();
        assert_eq!(plan.base_revision, agent.revision);
        assert_eq!(plan.changes.len(), 1);
        assert_eq!(plan.changes[0].path, "/sequences/main/height");
        let merged_again = app
            .merge_branches(&BranchMergeRequest {
                project_id: "p".into(),
                source_branch: "agent".into(),
                target_branch: "main".into(),
                expected_source_revision: next.revision,
                expected_target_revision: merged.transaction.revision,
                idempotency_key: "merge-agent-next".into(),
            })
            .unwrap();
        let main = app.snapshot("p").unwrap();
        assert_eq!(main.revision, merged_again.transaction.revision);
        assert_eq!(main.project.sequences[0].width, 800);
        assert_eq!(main.project.sequences[0].height, 400);
        assert_eq!(main.project.sequences[0].name, "Human main");
        let undo = opencut_api_contracts::collaboration::UndoRequest {
            project_id: "p".into(),
            expected_revision: main.revision.clone(),
            target_transaction_id: agent.transaction_id,
            idempotency_key: "undo-merged-branch-edit".into(),
        };
        assert!(app.plan_undo(&undo).unwrap().conflicts.is_empty());
        app.apply_undo(&undo).unwrap();
        let selectively_undone = app.snapshot("p").unwrap();
        assert_eq!(selectively_undone.project.sequences[0].width, 640);
        assert_eq!(selectively_undone.project.sequences[0].height, 400);
        assert_eq!(selectively_undone.project.sequences[0].name, "Human main");
    }

    #[test]
    fn divergent_field_and_stale_branch_head_reject_merge_without_mutation() {
        let mut app = EditorApplication::open(":memory:").unwrap();
        let initial = project(&mut app, "p");
        create(&mut app, &initial.revision, "agent");
        let agent = app
            .apply_branch_edit(&BranchEditRequest {
                project_id: "p".into(),
                branch_name: "agent".into(),
                request: edit(&initial.revision, "agent-name", Some("Agent"), None, None),
            })
            .unwrap();
        let human = app
            .apply(
                "p",
                &edit(&initial.revision, "human-name", Some("Human"), None, None),
            )
            .unwrap();
        let compare = BranchCompareRequest {
            project_id: "p".into(),
            source_branch: "agent".into(),
            target_branch: "main".into(),
            expected_source_revision: agent.revision.clone(),
            expected_target_revision: human.revision.clone(),
        };
        let plan = app.compare_branches(&compare).unwrap();
        assert_eq!(plan.conflicts[0].path, "/sequences/main/name");
        let merge = BranchMergeRequest {
            project_id: "p".into(),
            source_branch: "agent".into(),
            target_branch: "main".into(),
            expected_source_revision: agent.revision.clone(),
            expected_target_revision: human.revision.clone(),
            idempotency_key: "merge-conflict".into(),
        };
        assert_eq!(
            app.merge_branches(&merge).unwrap_err().code,
            "BRANCH_MERGE_CONFLICT"
        );
        assert_eq!(app.snapshot("p").unwrap().revision, human.revision);
        let stale = BranchMergeRequest {
            expected_source_revision: initial.revision,
            ..merge
        };
        assert_eq!(
            app.merge_branches(&stale).unwrap_err().code,
            "REVISION_CONFLICT"
        );
        assert_eq!(app.snapshot("p").unwrap().revision, human.revision);
    }

    #[test]
    fn schema_nineteen_upgrades_to_branch_storage() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("workspace.sqlite");
        let mut app = EditorApplication::open(&path).unwrap();
        let root = project(&mut app, "p");
        app.connection.execute_batch("DROP TABLE project_preset_grants; DROP TABLE project_edit_control; DROP TABLE project_merge_parents; DROP TABLE project_branches; UPDATE metadata SET value='19' WHERE key='schema_version';").unwrap();
        drop(app);
        let mut upgraded = EditorApplication::open(&path).unwrap();
        let branch = create(&mut upgraded, &root.revision, "agent");
        assert_eq!(branch.head_revision, root.revision);
        assert_eq!(upgraded.snapshot("p").unwrap().revision, root.revision);
    }

    #[test]
    fn scoped_branch_permissions_and_merge_audit_rollback() {
        let mut app = EditorApplication::open(":memory:").unwrap();
        let initial = project(&mut app, "p");
        create(&mut app, &initial.revision, "agent");
        let edit_input = BranchEditRequest {
            project_id: "p".into(),
            branch_name: "agent".into(),
            request: edit(&initial.revision, "branch-width", None, Some(800), None),
        };
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
        let secret = app.issue_credential("reader").unwrap();
        app.set_caller(app.authenticate(&secret.secret).unwrap());
        assert_eq!(
            app.list_branches(&BranchListRequest {
                project_id: "p".into()
            })
            .unwrap()
            .len(),
            2
        );
        assert_eq!(
            app.apply_branch_edit(&edit_input).unwrap_err().code,
            "NOT_AUTHORIZED"
        );
        app.set_caller(Caller::LocalOwner);
        app.set_principal_policy(PrincipalPolicy {
            principal_id: "editor".into(),
            expected_revision: "0".into(),
            enabled: true,
            grants: vec![ProjectGrant {
                project_id: "p".into(),
                actions: vec![ProjectAction::Read, ProjectAction::Edit],
            }],
        })
        .unwrap();
        let secret = app.issue_credential("editor").unwrap();
        app.set_caller(app.authenticate(&secret.secret).unwrap());
        let branch_edit = app.apply_branch_edit(&edit_input).unwrap();
        let request = BranchMergeRequest {
            project_id: "p".into(),
            source_branch: "agent".into(),
            target_branch: "main".into(),
            expected_source_revision: branch_edit.revision,
            expected_target_revision: initial.revision.clone(),
            idempotency_key: "merge".into(),
        };
        assert!(
            app.compare_branches(&compare_input(&request))
                .unwrap()
                .conflicts
                .is_empty()
        );
        app.set_caller(Caller::LocalOwner);
        app.connection.execute_batch("CREATE TRIGGER fail_branch_audit BEFORE INSERT ON audit_records WHEN NEW.action='project.branch_merged' BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
        assert_eq!(
            app.merge_branches(&request).unwrap_err().code,
            "STORAGE_ERROR"
        );
        assert_eq!(app.snapshot("p").unwrap().revision, initial.revision);
        let parents: i64 = app
            .connection
            .query_row("SELECT COUNT(*) FROM project_merge_parents", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(parents, 0);
        app.connection
            .execute_batch("DROP TRIGGER fail_branch_audit")
            .unwrap();
        assert_eq!(
            app.merge_branches(&request)
                .unwrap()
                .transaction
                .base_revision,
            initial.revision
        );
    }
}
