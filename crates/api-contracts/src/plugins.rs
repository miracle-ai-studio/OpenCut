use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PluginManifest {
    pub manifest_version: u32,
    pub id: String,
    pub version: String,
    pub display_name: String,
    pub plugin_api: String,
    pub runtime: RuntimeManifest,
    pub targets: Vec<String>,
    pub permissions: Vec<String>,
    pub contributes: Contributions,
    pub limits: PluginLimits,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeManifest {
    pub kind: String,
    pub entry: String,
    pub world: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Contributions {
    pub operations: Vec<PluginOperation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PluginOperation {
    pub name: String,
    pub version: u32,
    pub kind: String,
    pub input_schema: String,
    pub output_schema: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PluginLimits {
    pub memory_mib: u64,
    pub wall_time_ms: u64,
    pub output_bytes: usize,
    #[serde(default = "default_fuel")]
    pub fuel: u64,
}

fn default_fuel() -> u64 {
    10_000_000
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InstallInput {
    pub package_root: String,
    pub granted_permissions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PluginInput {
    pub id: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlanInput {
    pub id: String,
    pub version: String,
    pub operation_name: String,
    pub project_id: String,
    pub revision: String,
    pub parameters: Value,
}
