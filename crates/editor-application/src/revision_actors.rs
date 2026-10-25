//! Narrow read projection of committed provenance; full audit remains separately granted.
use opencut_api_contracts::revision_actors::*;
use opencut_contracts::EditorError;
pub trait RevisionActorsRead {
    /// Read permission and revision ownership must be checked in the same view.
    /// Missing historical provenance returns None; never infer a caller or source.
    fn revision_actors(&self, input: &RevisionActorsRequest)
    -> Result<RevisionActors, EditorError>;
}
pub fn read(
    store: &impl RevisionActorsRead,
    input: RevisionActorsRequest,
) -> Result<RevisionActors, EditorError> {
    let mut unique = std::collections::BTreeSet::new();
    if input.project_id.trim().is_empty()
        || input.revisions.is_empty()
        || input.revisions.len() > 100
        || input
            .revisions
            .iter()
            .any(|r| r.is_empty() || r.len() > 1024 || !unique.insert(r))
    {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Revision attribution needs 1..100 unique fixed revisions",
        ));
    }
    store.revision_actors(&input)
}
