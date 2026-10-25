//! Project plugins propose one ordinary complete edit through existing authority.
use super::{PlanProjectPlugin, ProjectPluginPlan, ProjectPluginRuntime};
use crate::{
    project_plugins as scoped,
    scene::{self, SceneCommitIntent, StoredProjectDocument},
};
use opencut_contracts::{EditorError, project_public::*, scene::EditorSnapshot};

fn invalid() -> EditorError {
    EditorError::new("INVALID_ARGUMENT", "Invalid project plugin plan")
}
pub fn plan(
    store: &(impl scoped::ProjectPluginStore + scene::public::ProjectPublicResources),
    runtime: &impl ProjectPluginRuntime,
    input: PlanProjectPlugin,
) -> Result<ProjectPluginPlan, EditorError> {
    crate::require_idempotency_key(&input.idempotency_key)?;
    if input.label.trim().is_empty()
        || input.label.len() > 4096
        || serde_json::to_vec(&input.plugin.parameters)
            .map_err(|_| invalid())?
            .len()
            > scoped::MAX_PARAMETERS
    {
        return Err(invalid());
    }
    let p = &input.plugin;
    let inspection = scoped::InspectInput {
        target: scoped::PluginTarget {
            project_id: p.project_id.clone(),
            id: p.id.clone(),
            version: p.version.clone(),
        },
        operation_name: p.operation_name.clone(),
    };
    let ticket = store.authorize_plugin(&inspection, true)?;
    let mut transaction = ApplyProject {
        branch_name: input.branch_name,
        project_id: p.project_id.clone(),
        expected_revision: p.revision.clone(),
        idempotency_key: input.idempotency_key,
        label: input.label,
        operations: vec![],
    };
    store.authorize_plan(SceneCommitIntent::PublicApply(&transaction))?;
    let source =
        scene::persistence::checked_head(store, SceneCommitIntent::PublicApply(&transaction))?;
    if source.revision != p.revision || source.document.project_id() != p.project_id {
        return Err(invalid());
    }
    let StoredProjectDocument::Editable(project) = source.document else {
        return Err(EditorError::new(
            "PROJECT_PROMOTION_REQUIRED",
            "Promote the historical project explicitly before planning edits",
        ));
    };
    opencut_editor_core::scene::validate_project_scene(&project)?;
    let snapshot = scene::public::project(&EditorSnapshot {
        project: *project,
        revision: source.revision,
    });
    if serde_json::to_vec(&snapshot).map_err(|_| invalid())?.len() > scoped::MAX_SNAPSHOT {
        return Err(invalid());
    }
    let proposed = runtime.propose_project_operations(p, &snapshot, &ticket.grant, &|| {
        store.revalidate_plugin(&inspection, true, &ticket)
    });
    store.revalidate_plugin(&inspection, true, &ticket)?;
    transaction.operations = proposed?;
    if transaction.operations.is_empty() || transaction.operations.len() > scoped::MAX_OPERATIONS {
        return Err(EditorError::new(
            "INVALID_PLUGIN",
            "Plugin must propose 1..32 operations",
        ));
    }
    let preview = scene::public::plan_public(store, &transaction)?;
    // Recheck after resource resolution and potentially expensive preparation.
    store.authorize_plan(SceneCommitIntent::PublicApply(&transaction))?;
    store.revalidate_plugin(&inspection, true, &ticket)?;
    Ok(ProjectPluginPlan {
        grant: ticket.grant,
        transaction,
        preview,
    })
}
