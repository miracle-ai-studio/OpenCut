use super::*;
use opencut_contracts::scene_bookmarks::BookmarkOperationV1;

pub(super) fn apply(
    project: &ProjectDocumentV4,
    operation: &BookmarkOperationV1,
) -> Result<ProjectDocumentV4, EditorError> {
    let proposed = opencut_editor_core::scene_bookmarks::plan_bookmarks_v4(
        project,
        std::slice::from_ref(operation),
    )?;
    let mut result = project.clone();
    for sequence in &mut result.sequences {
        sequence.bookmarks = proposed
            .iter()
            .filter(|b| b.sequence_id == sequence.id)
            .cloned()
            .collect();
    }
    Ok(result)
}
