//! Conservative planner ceilings for callers that do not own the native workspace.
use super::*;
use opencut_editor_application::project_plugins::{
    self as app, Grant, InspectInput, OperationView,
};
use std::io::Read;
pub struct ProjectPolicy<'a> {
    pub grant: &'a Grant,
    pub authorize: &'a dyn Fn() -> Result<(), EditorError>,
}
pub(super) fn limits(
    plugin: &mut InstalledWasmPlugin,
    policy: &ProjectPolicy<'_>,
) -> Result<(), EditorError> {
    if plugin.package_sha256 != policy.grant.package_sha256
        || plugin.manifest.id != policy.grant.target.id
        || plugin.manifest.version != policy.grant.target.version
    {
        return Err(opencut_editor_application::access::denied());
    }
    require_grant(plugin, "project.snapshot.read")?;
    require_grant(plugin, "edit.plan.propose")?;
    let cap = app::ceilings();
    let limits = &mut plugin.manifest.limits;
    limits.memory_mib = limits.memory_mib.min(cap.memory_mib);
    limits.wall_time_ms = limits.wall_time_ms.min(cap.wall_time_ms);
    limits.output_bytes = limits.output_bytes.min(cap.output_bytes);
    limits.fuel = limits.fuel.min(cap.fuel);
    (policy.authorize)()
}
pub(super) fn bounded_read(path: &Path, limit: usize) -> Result<Vec<u8>, EditorError> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(io_error)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() > limit {
        return Err(invalid("Project plugin input exceeds host byte ceiling"));
    }
    Ok(bytes)
}
pub(super) fn schema(path: &Path) -> Result<Value, EditorError> {
    fn local(value: &Value, depth: usize) -> bool {
        if depth > 64 {
            return false;
        }
        match value {
            Value::Object(map) => map.iter().all(|(k, v)| {
                (!matches!(k.as_str(), "$ref" | "$dynamicRef" | "$recursiveRef")
                    || v.as_str().is_some_and(|s| s.starts_with('#')))
                    && local(v, depth + 1)
            }),
            Value::Array(values) => values.iter().all(|v| local(v, depth + 1)),
            _ => true,
        }
    }
    let value: Value =
        serde_json::from_slice(&bounded_read(path, app::MAX_SCHEMA)?).map_err(json_error)?;
    if !local(&value, 0) {
        return Err(invalid(
            "Project plugin schemas require document-local references and bounded depth",
        ));
    }
    // Compile here as discovery must not advertise an unusable contract.
    jsonschema::validator_for(&value).map_err(|_| invalid("Invalid project plugin schema"))?;
    Ok(value)
}
impl PluginHost {
    pub fn inspect_project_operation(
        &self,
        input: &InspectInput,
        policy: &ProjectPolicy<'_>,
    ) -> Result<OperationView, EditorError> {
        app::validate_target(&input.target, &policy.grant.package_sha256)?;
        if input.target != policy.grant.target
            || !policy.grant.enabled
            || !policy.grant.operations.contains(&input.operation_name)
        {
            return Err(opencut_editor_application::access::denied());
        }
        let _lock = self.read_lock(&input.target.id, &input.target.version)?;
        (policy.authorize)()?;
        let mut plugin = self.inspect(&input.target.id, &input.target.version)?;
        validate_manifest(
            &plugin.manifest,
            &plugin.install_path.canonicalize().map_err(io_error)?,
        )?;
        limits(&mut plugin, policy)?;
        let op = plugin
            .manifest
            .contributes
            .operations
            .iter()
            .find(|op| op.name == input.operation_name)
            .ok_or_else(|| EditorError::new("NOT_FOUND", "Plugin contribution is unavailable"))?;
        let input_schema = schema(&plugin.install_path.join(&op.input_schema))?;
        let output_schema = schema(&plugin.install_path.join(&op.output_schema))?;
        Ok(OperationView {
            grant: policy.grant.clone(),
            operation_name: op.name.clone(),
            operation_version: op.version,
            input_schema,
            output_schema,
            limits: plugin.manifest.limits,
        })
    }
}
