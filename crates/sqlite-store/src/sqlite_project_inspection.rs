//! One authorized stored-revision read;no conversion,promotion or media capture.
use super::*;
use opencut_contracts::{
    project_inspection::HistoricalContentKind, project_inspection::InspectProject,
    scene::EditorSnapshot,
};
use opencut_editor_application::{
    project_inspection::{InspectionSource, ProjectInspectionRead},
    project_readonly::{self, StoredReadOnlySnapshot},
};
use sqlite_project_document::{ProjectStorageDocument, decode_stored_project};

impl ProjectInspectionRead for EditorApplication {
    fn read_inspection_source(
        &self,
        input: &InspectProject,
    ) -> Result<InspectionSource, EditorError> {
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
        sqlite_project_library::require_live(&tx, &input.project_id)?;
        let row:Option<(String,String)>=match &input.revision {
            Some(revision)=>tx.query_row("SELECT document_json,revision FROM revisions WHERE project_id=?1 AND revision=?2",
                params![input.project_id,revision],|row|Ok((row.get(0)?,row.get(1)?))).optional().map_err(storage_error)?,
            None=>tx.query_row("SELECT r.document_json,p.head_revision FROM projects p JOIN revisions r ON r.project_id=p.project_id AND r.revision=p.head_revision WHERE p.project_id=?1",
                [&input.project_id],|row|Ok((row.get(0)?,row.get(1)?))).optional().map_err(storage_error)?,
        };
        let (raw, revision) =
            row.ok_or_else(|| EditorError::new("NOT_FOUND", "Project revision does not exist"))?;
        if raw.len() > opencut_editor_application::gc::MAX_REFERENCE_DOCUMENT_BYTES {
            return Err(EditorError::new(
                "RESOURCE_TOO_LARGE",
                "Project metadata exceeds the reference budget",
            ));
        }
        let source = match decode_stored_project(&raw)? {
            ProjectStorageDocument::Legacy(base) => {
                if base.id != input.project_id {
                    return Err(EditorError::new(
                        "STORAGE_ERROR",
                        "Historical project identity differs from scope",
                    ));
                }
                InspectionSource::Historical {
                    base,
                    revision,
                    format: HistoricalContentKind::Base,
                    visual: vec![],
                    curves: vec![],
                }
            }
            ProjectStorageDocument::HistoricalVisual(document) => {
                if document.base.id != input.project_id {
                    return Err(EditorError::new(
                        "STORAGE_ERROR",
                        "Historical visual project identity differs from scope",
                    ));
                }
                InspectionSource::Historical {
                    base: document.base,
                    revision,
                    format: HistoricalContentKind::Visual,
                    visual: document.visual,
                    curves: document.keyframe_curves,
                }
            }
            ProjectStorageDocument::Editable(project) => {
                if project.id != input.project_id {
                    return Err(EditorError::new(
                        "STORAGE_ERROR",
                        "Editor project identity differs from scope",
                    ));
                }
                InspectionSource::Editor(EditorSnapshot { project, revision })
            }
            ProjectStorageDocument::ReadOnly(document) => {
                if document.id != input.project_id
                    || revision != project_readonly::revision(&document)?
                {
                    return Err(EditorError::new(
                        "STORAGE_ERROR",
                        "Preserved project identity differs from scope",
                    ));
                }
                InspectionSource::Preserved(StoredReadOnlySnapshot { document, revision })
            }
        };
        tx.commit().map_err(storage_error)?;
        Ok(source)
    }
}
