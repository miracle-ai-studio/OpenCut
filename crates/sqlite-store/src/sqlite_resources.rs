//! Explicit versioned, read-only resource projection. Legacy edit ports stay strict.
use super::*;
use opencut_editor_application::resources::{ContextSnapshot, ProjectContextRead};
impl ProjectContextRead for SqliteConnection {
    fn project_context(
        &self,
        project: &str,
        revision: Option<&str>,
    ) -> Result<ContextSnapshot, EditorError> {
        let tx = self.unchecked_transaction().map_err(storage_error)?;
        sqlite_access::require(&tx, &self.1, project, ProjectAction::Read)?;
        let (raw,revision):(String,String)=if let Some(revision)=revision {
            tx.query_row("SELECT document_json,revision FROM revisions WHERE project_id=?1 AND revision=?2",params![project,revision],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(storage_error)?
        } else {
            tx.query_row("SELECT r.document_json,p.head_revision FROM projects p JOIN revisions r ON r.revision=p.head_revision WHERE p.project_id=?1",[project],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(storage_error)?
        }.ok_or_else(||EditorError::new("NOT_FOUND","Project revision does not exist"))?;
        match sqlite_project_document::decode_versioned_project(&raw)? {
            sqlite_project_document::VersionedProjectDocument::Legacy(doc) if doc.id == project => {
                Ok(ContextSnapshot::Legacy(ProjectSnapshot {
                    project: doc,
                    revision,
                }))
            }
            sqlite_project_document::VersionedProjectDocument::V3(doc)
                if doc.base.id == project =>
            {
                Ok(ContextSnapshot::V3(
                    opencut_contracts::project_v3::ProjectSnapshotV3 {
                        project: doc,
                        revision,
                    },
                ))
            }
            sqlite_project_document::VersionedProjectDocument::V4(doc) if doc.id == project => {
                Err(EditorError::new(
                    "UNSUPPORTED_PROJECT_VERSION",
                    "Schema 4 content requires a schema 4 resource representation",
                ))
            }
            _ => Err(EditorError::new(
                "STORAGE_ERROR",
                "Project identity differs from resource scope",
            )),
        }
    }
}
