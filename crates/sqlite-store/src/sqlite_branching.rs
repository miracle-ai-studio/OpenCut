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
use opencut_contracts::branching::BranchView;

pub(super) fn branch_name(value: &str, main_allowed: bool) -> Result<(), EditorError> {
    opencut_editor_application::branching::validate_name(value, main_allowed)
}

pub(super) fn require_read(
    connection: &Connection,
    context: &opencut_editor_application::audit::ExecutionContext,
    project: &str,
) -> Result<(), EditorError> {
    sqlite_access::require(connection, context, project, ProjectAction::Read)
}
pub(super) fn require_edit(
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
        let head:String=connection.query_row(
            "SELECT p.head_revision FROM projects p JOIN revisions r ON r.project_id=p.project_id AND r.revision=p.head_revision WHERE p.project_id=?1",
            [project],|row|row.get(0),
        ).optional().map_err(storage_error)?.ok_or_else(||EditorError::new("NOT_FOUND","Project does not exist"))?;
        return Ok(BranchView {
            project_id: project.into(),
            branch_name: name.into(),
            head_revision: head,
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

pub(super) fn common_ancestor(
    connection: &Connection,
    project: &str,
    source: &str,
    target: &str,
) -> Result<String, EditorError> {
    let mut statement = connection
        .prepare("SELECT revision FROM revisions WHERE project_id=?1 ORDER BY rowid DESC")
        .map_err(storage_error)?;
    let rows = statement
        .query_map([project], |row| row.get::<_, String>(0))
        .map_err(storage_error)?;
    opencut_editor_application::branching::common_ancestor(
        source,
        target,
        rows.map(|row| row.map_err(storage_error)),
        |revision| parents(connection, project, revision),
    )
}

#[cfg(test)]
mod historical;
