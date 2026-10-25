//! One installed-package surface. Package kinds describe real capabilities,
//! while exact manifest/dependency identities remain artifact metadata.
use crate::{plugins::PluginManifest, presets::PresetManifest};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PluginPackageKind {
    Wasm,
    Presets,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InstallPluginPackage {
    pub package_root: String,
    #[serde(default)]
    pub granted_permissions: BTreeSet<String>,
    /// Present for a reviewed package restore. An incorrect pin cannot publish.
    pub expected_package_sha256: Option<String>,
}
/// A reviewed restore action always pins bytes. Its JSON is accepted by the
/// single install operation without weakening this result-side requirement.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PinnedPluginPackage {
    pub package_root: String,
    pub granted_permissions: Vec<String>,
    pub expected_package_sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InspectPluginPackage {
    pub kind: PluginPackageKind,
    pub id: String,
    pub version: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum InstalledPluginPackage {
    Wasm {
        manifest: PluginManifest,
        package_sha256: String,
        granted_permissions: BTreeSet<String>,
    },
    Presets {
        manifest: PresetManifest,
        package_sha256: String,
    },
}
