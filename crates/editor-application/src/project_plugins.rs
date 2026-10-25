//! Authorization tickets are native application values, never client claims.
mod planning;
use crate::access;
use opencut_api_contracts::plugins::{PlanInput, PluginLimits};
pub use opencut_api_contracts::project_plugins::*;
use opencut_contracts::{
    EditorError,
    project_public::{ProjectEditOperation, PublicProjectSnapshot},
};
pub use planning::plan;
pub const MAX_GRANTS: usize = 64;
pub const MAX_OPERATIONS: usize = 32;
pub const MAX_PARAMETERS: usize = 256 * 1024;
pub const MAX_SNAPSHOT: usize = 4 * 1024 * 1024;
pub const MAX_SCHEMA: usize = 256 * 1024;
pub const MAX_COMPONENT: usize = 16 * 1024 * 1024;
pub const MAX_CONCURRENT: usize = 4;
pub fn ceilings() -> PluginLimits {
    PluginLimits {
        memory_mib: 64,
        wall_time_ms: 1000,
        output_bytes: 256 * 1024,
        fuel: 5_000_000,
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Authorization {
    pub grant: Grant,
    pub principal_revision: Option<String>,
}
pub trait ProjectPluginStore {
    fn set_plugin_grant(&mut self, input: &SetGrant) -> Result<Grant, EditorError>;
    fn plugin_grants(&self, project: &str) -> Result<Vec<Grant>, EditorError>;
    fn authorize_plugin(
        &self,
        input: &InspectInput,
        execute: bool,
    ) -> Result<Authorization, EditorError>;
    fn revalidate_plugin(
        &self,
        input: &InspectInput,
        execute: bool,
        expected: &Authorization,
    ) -> Result<(), EditorError> {
        if self.authorize_plugin(input, execute)? != *expected {
            return Err(access::denied());
        }
        Ok(())
    }
}
pub trait ProjectPluginRuntime {
    /// Adapter holds the package version lock, checks hash/global grants, then calls authorize.
    fn inspect_project_operation(
        &self,
        input: &InspectInput,
        grant: &Grant,
        authorize: &dyn Fn() -> Result<(), EditorError>,
    ) -> Result<OperationView, EditorError>;
    fn propose_project_operations(
        &self,
        input: &PlanInput,
        snapshot: &PublicProjectSnapshot,
        grant: &Grant,
        authorize: &dyn Fn() -> Result<(), EditorError>,
    ) -> Result<Vec<ProjectEditOperation>, EditorError>;
}
pub fn validate_target(target: &PluginTarget, hash: &str) -> Result<(), EditorError> {
    if target.project_id.is_empty()
        || target.project_id.len() > 1024
        || target.project_id.chars().any(char::is_control)
    {
        return Err(invalid());
    }
    crate::maintenance::validate_object(
        &crate::maintenance::plugin_publication::Object::PluginPackage {
            plugin_id: target.id.clone(),
            version: target.version.clone(),
            package_sha256: hash.into(),
        },
    )
}
pub fn validate_grant(input: &SetGrant) -> Result<(), EditorError> {
    validate_target(&input.target, &input.package_sha256)?;
    let revision = input
        .expected_revision
        .parse::<i64>()
        .ok()
        .filter(|n| *n >= 0 && *n < i64::MAX && n.to_string() == input.expected_revision);
    if revision.is_none()
        || input.operations.len() > MAX_OPERATIONS
        || (input.enabled && input.operations.is_empty())
        || input.idempotency_key.is_empty()
        || input.idempotency_key.len() > 1024
    {
        return Err(invalid());
    }
    for op in &input.operations {
        if op.len() > 512
            || !op.starts_with(&format!("{}.", input.target.id))
            || !op.split('.').all(|s| {
                !s.is_empty()
                    && s.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            })
        {
            return Err(invalid());
        }
    }
    Ok(())
}
fn invalid() -> EditorError {
    EditorError::new("INVALID_ARGUMENT", "Invalid project plugin grant")
}
/// Preserve stable classification but never native paths, schema values or WASM backtraces.
pub fn public_error(error: EditorError) -> EditorError {
    EditorError::new(
        error.code,
        "Project plugin operation failed; check current project rights, grant, package and parameters",
    )
}
pub fn inspect(
    store: &impl ProjectPluginStore,
    runtime: &impl ProjectPluginRuntime,
    input: InspectInput,
) -> Result<OperationView, EditorError> {
    let ticket = store.authorize_plugin(&input, false)?;
    let result = runtime.inspect_project_operation(&input, &ticket.grant, &|| {
        store.revalidate_plugin(&input, false, &ticket)
    });
    store.revalidate_plugin(&input, false, &ticket)?;
    result
}
