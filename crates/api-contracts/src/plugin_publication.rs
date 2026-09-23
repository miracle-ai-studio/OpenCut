//! Plugin publication has its own stream; existing media query DTOs remain stable.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Object {
    PluginPackage {
        plugin_id: String,
        version: String,
        package_sha256: String,
    },
    PluginGrants {
        plugin_id: String,
        version: String,
        package_sha256: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    PluginPackagePublish,
    PluginGrantsUpdate,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Coverage {
    RecordedPluginPublicationsSinceSchema7,
}
pub type Intent = crate::maintenance::PublicationIntent<Object, Action>;
pub type Event = crate::maintenance::MaintenanceEvent<Object, Action>;
pub type Target = crate::maintenance::MaintenanceTarget<Object>;
pub type Query = crate::maintenance::MaintenanceQuery<Object>;
pub type Page = crate::maintenance::MaintenancePage<Object, Action, Coverage>;
pub type ReconcileInput = crate::maintenance::ReconcileInput<Object>;
pub type ReconcileResult = crate::maintenance::ReconcileResult<Object>;
