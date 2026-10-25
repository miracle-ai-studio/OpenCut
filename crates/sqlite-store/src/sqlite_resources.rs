//! Read-only resource projection over the original authorized revision authority.
use super::*;
use opencut_editor_application::resources::{ContextSnapshot, ProjectContextRead};
impl ProjectContextRead for EditorApplication {
    fn project_context(
        &self,
        project: &str,
        revision: Option<&str>,
        sequence: Option<&str>,
    ) -> Result<ContextSnapshot, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        sqlite_access::require(&tx, &self.connection.1, project, ProjectAction::Read)?;
        let (raw,revision):(String,String)=if let Some(revision)=revision {
            tx.query_row("SELECT document_json,revision FROM revisions WHERE project_id=?1 AND revision=?2",params![project,revision],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(storage_error)?
        } else {
            tx.query_row("SELECT r.document_json,p.head_revision FROM projects p JOIN revisions r ON r.revision=p.head_revision WHERE p.project_id=?1",[project],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(storage_error)?
        }.ok_or_else(||EditorError::new("NOT_FOUND","Project revision does not exist"))?;
        match sqlite_project_document::decode_stored_project(&raw)? {
            sqlite_project_document::ProjectStorageDocument::Legacy(doc) if doc.id == project => {
                Ok(ContextSnapshot::Legacy(ProjectSnapshot {
                    project: doc,
                    revision,
                }))
            }
            sqlite_project_document::ProjectStorageDocument::HistoricalVisual(doc)
                if doc.base.id == project =>
            {
                Ok(ContextSnapshot::HistoricalVisual(
                    opencut_contracts::historical_visual::HistoricalVisualSnapshot {
                        project: doc,
                        revision,
                    },
                ))
            }
            sqlite_project_document::ProjectStorageDocument::ReadOnly(doc) if doc.id == project => {
                Err(opencut_editor_application::project_readonly::readonly_error())
            }
            sqlite_project_document::ProjectStorageDocument::Editable(doc) if doc.id == project => {
                // Close the observation transaction before the shared read ports
                // create their own authorized snapshots, all pinned to this revision.
                drop(tx);
                opencut_editor_application::resources::project_context::read(
                    self, project, &revision, sequence,
                )
                .map(ContextSnapshot::Current)
            }
            _ => Err(EditorError::new(
                "STORAGE_ERROR",
                "Project identity differs from resource scope",
            )),
        }
    }
}
