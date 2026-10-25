//! One creation use case,with private original-receipt replay.
use crate::{
    require_idempotency_key,
    scene::{self, SceneCommitIntent, SceneStore},
};
use opencut_contracts::{
    EditorError,
    project_catalog::{CreateProjectRequest, ProjectCreated},
};

pub trait ProjectCreationStore: SceneStore {
    /// Authorize and read exact historical creation receipts without writing or
    /// projecting their documents into a different stored format.
    fn replay_project_creation(
        &self,
        request: &CreateProjectRequest,
    ) -> Result<Option<ProjectCreated>, EditorError>;
}
pub fn create(
    store: &mut impl ProjectCreationStore,
    request: &CreateProjectRequest,
) -> Result<ProjectCreated, EditorError> {
    require_idempotency_key(&request.idempotency_key)?;
    store.authorize_project(SceneCommitIntent::Create(request))?;
    if let Some(receipt) = store.replay_project_creation(request)? {
        return Ok(receipt);
    }
    let snapshot = scene::create(store, request)?;
    Ok(ProjectCreated {
        project_id: snapshot.project.id,
        revision: snapshot.revision,
    })
}
