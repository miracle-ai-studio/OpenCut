//! Plugin application use cases. A runtime can propose operations, never a trusted plan or a write.
use crate::projects::ProjectRead;
pub use opencut_api_contracts::plugins::{
    InstallInput, InstalledPlugin, PlanInput, PlanInvocation, PluginInput,
};
use opencut_contracts::{EditorError, Operation, ProjectSnapshot, TransactionRequest};
pub trait PluginCatalog {
    fn list(&self) -> Result<Vec<InstalledPlugin>, EditorError>;
    fn inspect(&self, input: &PluginInput) -> Result<InstalledPlugin, EditorError>;
    fn install(&self, input: InstallInput) -> Result<InstalledPlugin, EditorError>;
}
pub trait PluginRuntime {
    /// Runtime enforces package identity, grants, ABI, schemas and limits.
    /// It receives only an immutable snapshot and parameters, no store or commit capability.
    fn propose(
        &self,
        input: &PlanInput,
        snapshot: &ProjectSnapshot,
    ) -> Result<Vec<Operation>, EditorError>;
}
pub fn list(catalog: &impl PluginCatalog) -> Result<Vec<InstalledPlugin>, EditorError> {
    catalog.list()
}
pub fn inspect(
    catalog: &impl PluginCatalog,
    input: PluginInput,
) -> Result<InstalledPlugin, EditorError> {
    catalog.inspect(&input)
}
pub fn install(
    catalog: &impl PluginCatalog,
    input: InstallInput,
) -> Result<InstalledPlugin, EditorError> {
    catalog.install(input)
}
pub fn plan(
    projects: &impl ProjectRead,
    runtime: &impl PluginRuntime,
    input: PlanInput,
) -> Result<PlanInvocation, EditorError> {
    // Owned snapshot read is complete before the runtime executes; no write port is supplied.
    let snapshot = projects.revision(&input.project_id, &input.revision)?;
    let operations = runtime.propose(&input, &snapshot)?;
    plan_operations(
        &input.id,
        &input.version,
        &input.operation_name,
        &snapshot,
        operations,
    )
}
pub fn plan_operations(
    id: &str,
    version: &str,
    operation_name: &str,
    snapshot: &ProjectSnapshot,
    operations: Vec<Operation>,
) -> Result<PlanInvocation, EditorError> {
    let request = TransactionRequest {
        expected_revision: snapshot.revision.clone(),
        idempotency_key: format!("plugin-plan:{id}:{operation_name}"),
        label: format!("Plan from {operation_name}"),
        operations: operations.clone(),
    };
    let plan=opencut_editor_core::plan_transaction(snapshot,&request).map_err(|cause| {
        EditorError::new("INVALID_PLUGIN","Plugin proposed operations that cannot be applied to the requested snapshot")
            .with_details(serde_json::json!({"plugin_id":id,"plugin_version":version,"operation_name":operation_name,"cause":cause}))
    })?;
    Ok(PlanInvocation { operations, plan })
}
