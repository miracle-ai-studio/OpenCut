//! Attribution reads only principal/source, never audit request or credential data.
use super::*;
use opencut_api_contracts::{audit::Actor, revision_actors::*};
impl opencut_editor_application::revision_actors::RevisionActorsRead for EditorApplication {
    fn revision_actors(
        &self,
        input: &RevisionActorsRequest,
    ) -> Result<RevisionActors, EditorError> {
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
        let mut actors = std::collections::BTreeMap::new();
        for revision in &input.revisions {
            let exists: bool = tx
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM revisions WHERE project_id=?1 AND revision=?2)",
                    params![input.project_id, revision],
                    |r| r.get(0),
                )
                .map_err(storage_error)?;
            if !exists {
                return Err(EditorError::new(
                    "NOT_FOUND",
                    "Revision is absent from this project",
                ));
            }
            let stored:Option<String>=tx.query_row("SELECT actor_json FROM audit_records WHERE project_id=?1 AND revision=?2 AND action IN ('project.v4_created','project.v4_copied','project.v4_edited','project.v4_promoted','project.v4_restored','project.v4_selective_undo','project.v4_selective_redo','subtitle.v4_generated','subtitle.v4_imported') ORDER BY sequence LIMIT 1",params![input.project_id,revision],|r|r.get(0)).optional().map_err(storage_error)?;
            let actor = stored
                .map(|value| {
                    serde_json::from_str::<Actor>(&value)
                        .map(|actor| RevisionActor {
                            principal_id: actor.principal_id,
                            source: actor.source,
                        })
                        .map_err(|_| {
                            EditorError::new(
                                "INVALID_STORED_RESULT",
                                "Committed actor data is invalid",
                            )
                        })
                })
                .transpose()?;
            actors.insert(revision.clone(), actor);
        }
        tx.commit().map_err(storage_error)?;
        Ok(RevisionActors {
            project_id: input.project_id.clone(),
            actors,
        })
    }
}
