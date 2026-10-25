//! Classify one authorized immutable source; no job or output is created.
use super::*;
use opencut_contracts::{
    ProjectSnapshot, historical_visual::HistoricalVisualSnapshot, scene::EditorSnapshot,
};
use opencut_editor_application::{
    access::SourceAccess,
    media_video_jobs::AuthorizedVideoSnapshot,
    render::{AuthorizedPreflight, PreflightRead, PreflightSource},
    scene::StoredProjectDocument,
};
impl PreflightRead for EditorApplication {
    fn read_preflight(
        &self,
        project: &str,
        revision: &str,
    ) -> Result<AuthorizedPreflight, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        sqlite_scene_jobs::require_scope(&tx, &self.connection.1, project)?;
        let fixed = sqlite_scene::load_source(&tx, project, Some(revision))?;
        let source = match fixed.document {
            StoredProjectDocument::Legacy(document) => {
                PreflightSource::Historical(ProjectSnapshot {
                    project: *document,
                    revision: fixed.revision,
                })
            }
            StoredProjectDocument::HistoricalVisual(document) => {
                PreflightSource::HistoricalVisual(HistoricalVisualSnapshot {
                    project: *document,
                    revision: fixed.revision,
                })
            }
            StoredProjectDocument::Editable(document) => {
                PreflightSource::Editor(AuthorizedVideoSnapshot {
                    snapshot: EditorSnapshot {
                        project: *document,
                        revision: fixed.revision,
                    },
                    authority: sqlite_scene_jobs::authority(&self.connection.1),
                    processing_policy: sqlite_media_video_jobs::policy(),
                })
            }
        };
        let sources = if self.connection.1.caller().is_owner() {
            SourceAccess::OwnerPaths
        } else {
            SourceAccess::ProjectContent
        };
        tx.commit().map_err(storage_error)?;
        Ok(AuthorizedPreflight { source, sources })
    }
}
