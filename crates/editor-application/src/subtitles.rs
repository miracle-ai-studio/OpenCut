//! One authorized fixed-revision SRT/ASS export;original writers remain unchanged.
use crate::scene::{SceneRead, StoredProjectDocument};
use opencut_api_contracts::subtitle_exchange::{SubtitleExport, SubtitleExportReceipt};
use opencut_contracts::EditorError;
pub fn export(
    store: &impl SceneRead,
    input: &SubtitleExport,
) -> Result<SubtitleExportReceipt, EditorError> {
    let source = store.revision_scene_source(&input.project_id, &input.revision)?;
    if source.revision != input.revision || source.document.project_id() != input.project_id {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Subtitle export fixed source changed",
        ));
    }
    match source.document {
        StoredProjectDocument::Legacy(project) => crate::subtitle_exchange::export_subtitles(
            &opencut_contracts::ProjectSnapshot {
                project: *project,
                revision: source.revision,
            },
            input,
        ),
        StoredProjectDocument::HistoricalVisual(project) => {
            crate::subtitle_exchange::export_subtitles(
                &opencut_contracts::ProjectSnapshot {
                    project: project.base,
                    revision: source.revision,
                },
                input,
            )
        }
        StoredProjectDocument::Editable(project) => {
            opencut_editor_core::scene::validate_project_scene(&project)?;
            crate::captions::export(
                &opencut_contracts::scene::EditorSnapshot {
                    project: *project,
                    revision: source.revision,
                },
                input,
            )
        }
    }
}
