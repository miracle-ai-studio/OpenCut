use super::*;
use opencut_contracts::scene_bookmarks::BookmarkOperation;

pub(super) fn apply(
    project: &EditorDocument,
    operation: &BookmarkOperation,
) -> Result<EditorDocument, EditorError> {
    let proposed = opencut_editor_core::scene_bookmarks::plan_bookmarks_scene(
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
