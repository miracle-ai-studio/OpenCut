//! V4 history reads retain the existing revision/journal authority. Publication
//! goes through the same prepared V4 commit, resource fence and SQL transaction.
use super::*;
use opencut_contracts::{project_history::*, scene_ops::*};
use opencut_editor_application::gc::{
    MAX_REFERENCE_BYTES, MAX_REFERENCE_DOCUMENT_BYTES, MAX_REFERENCE_MS, MAX_REFERENCE_ROWS,
};
use opencut_editor_application::scene::StoredProjectRevision;
use opencut_editor_application::scene::history::{
    self, HistoryEntry, HistoryEntryKind, HistorySource, HistoryTransition, ProjectHistoryRead,
};
use std::collections::HashSet;

pub(crate) mod journal;
#[cfg(test)]
mod tests;

type HistoryRow = (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
    Option<String>,
);

fn source(
    connection: &Connection,
    project: &str,
    revision: Option<&str>,
) -> Result<StoredProjectRevision, EditorError> {
    sqlite_scene::load_source(connection, project, revision)
}
fn require_read(
    connection: &Connection,
    context: &opencut_editor_application::audit::ExecutionContext,
    project: &str,
) -> Result<(), EditorError> {
    sqlite_access::require(connection, context, project, ProjectAction::Read)?;
    sqlite_project_library::require_live(connection, project)
}

#[derive(Default)]
struct Budget {
    bytes: usize,
    rows: usize,
}
impl Budget {
    fn observe(&mut self, values: &[&str], start: std::time::Instant) -> Result<(), EditorError> {
        self.rows += 1;
        for value in values {
            self.bytes = self.bytes.saturating_add(value.len());
            if value.len() > MAX_REFERENCE_DOCUMENT_BYTES {
                return Err(budget_error());
            }
        }
        if self.bytes > MAX_REFERENCE_BYTES
            || self.rows > MAX_REFERENCE_ROWS
            || start.elapsed() > Duration::from_millis(MAX_REFERENCE_MS)
        {
            return Err(budget_error());
        }
        Ok(())
    }
}
fn budget_error() -> EditorError {
    EditorError::new(
        "RESOURCE_TOO_LARGE",
        "V4 history exceeds the bounded reference traversal budget",
    )
}

impl ProjectHistoryRead for EditorApplication {
    fn read_scene_history(
        &self,
        project: &str,
        revision: Option<&str>,
    ) -> Result<HistorySource, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        require_read(&tx, &self.connection.1, project)?;
        let source = source(&tx, project, revision)?;
        let mut cursor = Some(source.revision.clone());
        let mut visited = HashSet::new();
        let mut entries = Vec::new();
        let mut budget = Budget::default();
        let mut validated_parent: Option<(String, u32)> = None;
        let start = std::time::Instant::now();
        {
            let mut statement = tx.prepare("SELECT r.parent_revision,r.transaction_id,t.base_revision,t.revision,t.request_json,r.document_json,p.document_json
                FROM revisions r LEFT JOIN editor_transactions t ON t.project_id=r.project_id AND t.transaction_id=r.transaction_id
                LEFT JOIN revisions p ON p.project_id=r.project_id AND p.revision=r.parent_revision
                WHERE r.project_id=?1 AND r.revision=?2").map_err(storage_error)?;
            while let Some(revision) = cursor {
                if !visited.insert(revision.clone()) {
                    return Err(EditorError::new(
                        "STORAGE_ERROR",
                        "History contains a revision cycle",
                    ));
                }
                let row: HistoryRow = statement
                    .query_row(params![project, revision], |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                            row.get(5)?,
                            row.get(6)?,
                        ))
                    })
                    .optional()
                    .map_err(storage_error)?
                    .ok_or_else(|| {
                        EditorError::new(
                            "STORAGE_ERROR",
                            "History revision is outside its project scope",
                        )
                    })?;
                let (parent, transaction, base, journal_revision, raw, document, parent_document) =
                    row;
                budget.observe(
                    &[
                        &document,
                        raw.as_deref().unwrap_or(""),
                        parent_document.as_deref().unwrap_or(""),
                    ],
                    start,
                )?;
                let after_schema = match validated_parent.take() {
                    Some((id, schema)) if id == revision => schema,
                    _ => journal::document_schema(&document, project)?,
                };
                cursor = parent.clone();
                if let Some(transaction_id) = transaction {
                    if base.as_ref() != parent.as_ref()
                        || journal_revision.as_ref() != Some(&revision)
                    {
                        return Err(EditorError::new(
                            "STORAGE_ERROR",
                            "History transaction revision edges differ",
                        ));
                    }
                    let base_revision = base.ok_or_else(|| {
                        EditorError::new(
                            "STORAGE_ERROR",
                            "History transaction has no base revision",
                        )
                    })?;
                    let raw = raw.ok_or_else(|| {
                        EditorError::new("STORAGE_ERROR", "History transaction journal is missing")
                    })?;
                    let parent_document = parent_document.ok_or_else(|| {
                        EditorError::new("STORAGE_ERROR", "History parent document is missing")
                    })?;
                    let before_schema = journal::document_schema(&parent_document, project)?;
                    validated_parent = Some((base_revision.clone(), before_schema));
                    entries.push(journal::entry_with_schemas(
                        project,
                        &transaction_id,
                        &base_revision,
                        &revision,
                        &raw,
                        &parent_document,
                        &document,
                        before_schema,
                        after_schema,
                    )?);
                } else if parent.is_some() {
                    return Err(EditorError::new(
                        "STORAGE_ERROR",
                        "Noninitial history revision has no transaction",
                    ));
                }
            }
        }
        if start.elapsed() > Duration::from_millis(MAX_REFERENCE_MS) {
            return Err(budget_error());
        }
        entries.reverse();
        tx.commit().map_err(storage_error)?;
        Ok(HistorySource { source, entries })
    }
    fn merged_history_transition(
        &self,
        project: &str,
        head: &str,
        transaction: &str,
    ) -> Result<Option<HistoryTransition>, EditorError> {
        let transition = match self.read_scene_history_transition(project, transaction) {
            Ok(value) => value,
            Err(error) if error.code == "NOT_FOUND" => return Ok(None),
            Err(error) => return Err(error),
        };
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        require_read(&tx, &self.connection.1, project)?;
        let ancestor =
            sqlite_branching::common_ancestor(&tx, project, head, &transition.after.revision)?;
        let reachable = ancestor == transition.after.revision;
        tx.commit().map_err(storage_error)?;
        Ok(reachable.then_some(transition))
    }
    fn read_scene_history_transition(
        &self,
        project: &str,
        transaction: &str,
    ) -> Result<HistoryTransition, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        require_read(&tx, &self.connection.1, project)?;
        let row: Option<(String,String,String)> = tx.query_row("SELECT base_revision,revision,request_json FROM editor_transactions WHERE project_id=?1 AND transaction_id=?2",
            params![project,transaction], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional().map_err(storage_error)?;
        let (base, revision, raw) = row.ok_or_else(|| {
            EditorError::new("NOT_FOUND", "Transaction does not belong to this project")
        })?;
        let before = source(&tx, project, Some(&base))?;
        let after = source(&tx, project, Some(&revision))?;
        let parent: Option<String> = tx.query_row("SELECT parent_revision FROM revisions WHERE project_id=?1 AND revision=?2 AND transaction_id=?3", params![project,revision,transaction], |row| row.get(0)).optional().map_err(storage_error)?
            .ok_or_else(|| EditorError::new("STORAGE_ERROR", "Transaction has no matching revision"))?;
        if parent.as_deref() != Some(&base) {
            return Err(EditorError::new(
                "STORAGE_ERROR",
                "Transaction parent differs from base revision",
            ));
        }
        let before_raw: String = tx
            .query_row(
                "SELECT document_json FROM revisions WHERE project_id=?1 AND revision=?2",
                params![project, base],
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        let after_raw: String = tx
            .query_row(
                "SELECT document_json FROM revisions WHERE project_id=?1 AND revision=?2",
                params![project, revision],
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        Budget::default().observe(&[&raw, &before_raw, &after_raw], std::time::Instant::now())?;
        let entry = journal::entry(
            project,
            transaction,
            &base,
            &revision,
            &raw,
            &before_raw,
            &after_raw,
        )?;
        tx.commit().map_err(storage_error)?;
        Ok(HistoryTransition {
            entry,
            before,
            after,
        })
    }
}
impl EditorApplication {
    pub fn plan_project_undo(&self, request: &UndoRequest) -> Result<HistoryPlan, EditorError> {
        history::plan_undo(self, request)
    }
    pub fn plan_project_redo(&self, request: &RedoRequest) -> Result<HistoryPlan, EditorError> {
        history::plan_redo(self, request)
    }
    pub fn apply_project_undo(
        &mut self,
        request: &UndoRequest,
    ) -> Result<HistoryCommit, EditorError> {
        history::undo(self, request)
    }
    pub fn apply_project_redo(
        &mut self,
        request: &RedoRequest,
    ) -> Result<HistoryCommit, EditorError> {
        history::redo(self, request)
    }
    pub fn editor_history_navigation(
        &self,
        request: &HistoryNavigationInput,
    ) -> Result<HistoryNavigation, EditorError> {
        history::navigate(self, request)
    }
}
