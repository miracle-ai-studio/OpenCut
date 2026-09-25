//! One installed declarative package can plan against either project version.
use crate::plugin_v2::{self, PresetCatalog, PresetGrantStore};
use opencut_api_contracts::{
    plugin_v2::{PresetInput, PresetPlanInput},
    plugin_v3::PresetPlanV3,
};
use opencut_contracts::{
    EditorError, ProjectSnapshot,
    project_v3::ProjectSnapshotV3,
    project_v3_ops::{ApplyV3Project, VisualOperation},
};

pub trait PresetV3ProjectRead {
    fn preset_v3_snapshot(
        &self,
        project: &str,
        revision: &str,
    ) -> Result<ProjectSnapshotV3, EditorError>;
}
fn invalid() -> EditorError {
    EditorError::new("INVALID_PRESET", "Invalid schema 3 preset plan")
}
pub fn plan(
    store: &(impl PresetGrantStore + PresetV3ProjectRead),
    catalog: &impl PresetCatalog,
    input: PresetPlanInput,
) -> Result<PresetPlanV3, EditorError> {
    if !plugin_v2::valid_package_id(&input.target.id)
        || !plugin_v2::valid_version(&input.target.version)
        || !plugin_v2::valid_name(&input.preset_name)
        || serde_json::to_vec(&input).map_err(|_| invalid())?.len()
            > plugin_v2::MAX_PARAMETERS_BYTES
    {
        return Err(invalid());
    }
    let inspection = PresetInput {
        target: input.target.clone(),
        preset_name: input.preset_name.clone(),
    };
    let ticket = store.authorize_preset(&inspection, true)?;
    let snapshot = store.preset_v3_snapshot(&input.target.project_id, &input.revision)?;
    if snapshot.project.base.id != input.target.project_id
        || snapshot.revision != input.revision
        || serde_json::to_vec(&snapshot).map_err(|_| invalid())?.len()
            > plugin_v2::MAX_SNAPSHOT_BYTES
    {
        return Err(invalid());
    }
    opencut_editor_core::project_v3::validate_project_v3(&snapshot.project)?;
    let (declaration, spec) = catalog.load_spec(&inspection, &ticket.grant.package_sha256)?;
    store.revalidate_preset(&inspection, true, &ticket)?;
    let base = ProjectSnapshot {
        project: snapshot.project.base.clone(),
        revision: snapshot.revision.clone(),
    };
    let operations: Vec<_> =
        plugin_v2::expand(spec, &input.parameters, &input.asset_bindings, &base)?
            .into_iter()
            .map(|operation| VisualOperation::Base { operation })
            .collect();
    let plan = crate::project_v3::plan_apply(
        &snapshot.project,
        &ApplyV3Project {
            project_id: input.target.project_id,
            expected_revision: input.revision,
            idempotency_key: format!("preset-plan:{}:{}", input.target.id, input.preset_name),
            label: format!("Preset {}", input.preset_name),
            operations: operations.clone(),
        },
    )?;
    store.revalidate_preset(&inspection, true, &ticket)?;
    Ok(PresetPlanV3 {
        package_sha256: ticket.grant.package_sha256,
        spec_sha256: declaration.spec_sha256,
        grant_revision: ticket.grant.revision,
        operations,
        plan,
    })
}
