//! The sole project exchange contract. Scope and storage describe content, not
//! development generations. Permission snapshots are proposals, never authority.
use opencut_contracts::font_derived::FontDerivedSource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PackageMode {
    Linked,
    SelfContained,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackageToolLock {
    pub name: String,
    pub version: String,
    pub binary_sha256: String,
}

/// Fully parameterized calls that an Owner may review and apply after import.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "tool", content = "input", deny_unknown_fields)]
pub enum PluginGrantRestoreAction {
    #[serde(rename = "opencut_plugin_install")]
    Install(crate::plugin_packages::PinnedPluginPackage),
    #[serde(rename = "opencut_project_plugin_grant")]
    WasmGrant(crate::project_plugins::SetGrant),
    #[serde(rename = "opencut_plugin_grant")]
    PresetGrant(crate::presets::SetGrant),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PackageScope {
    SavedRevision,
    History,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportPackage {
    pub project_id: String,
    /// Fixed revision; history capture also requires this to be the main head.
    pub revision: String,
    pub destination: String,
    pub mode: PackageMode,
    pub scope: PackageScope,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PackageLocationPurpose {
    Save,
    Open,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PackageDialogLanguage {
    #[default]
    En,
    Zh,
}

/// Interactive owner-local selection only. No package is written or imported.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChoosePackageLocation {
    pub purpose: PackageLocationPurpose,
    pub suggested_name: Option<String>,
    #[serde(default)]
    pub language: PackageDialogLanguage,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackageLocation {
    /// Owner-local path selected by the user. None means the user cancelled.
    pub path: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InspectPackage {
    pub package_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImportPackage {
    pub package_path: String,
    pub target_project_id: String,
    pub expected_manifest_sha256: String,
    pub idempotency_key: String,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PluginKind {
    Wasm,
    Presets,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    content = "policy",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum PluginPolicy {
    Wasm(crate::project_plugins::Grant),
    Presets(crate::presets::Grant),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PluginBundle {
    pub kind: PluginKind,
    pub id: String,
    pub version: String,
    pub package_sha256: String,
    pub path: String,
    pub proposed_global_permissions: BTreeSet<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackageBlob {
    pub content_hash: String,
    pub size_bytes: u64,
    /// Self-contained resources use blobs/<sha256>. Linked resources carry an
    /// owner-local file hint and are checked again before import captures them.
    pub location: BlobLocation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    content = "path",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum BlobLocation {
    Bundled(String),
    Linked(String),
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackageManifest {
    pub source_project_id: String,
    pub source_revision: String,
    pub mode: PackageMode,
    pub scope: PackageScope,
    pub archive_path: String,
    pub archive_sha256: String,
    pub revision_count: usize,
    pub transaction_count: usize,
    pub branch_count: usize,
    pub merge_count: usize,
    pub asset_version_count: usize,
    pub font_version_count: usize,
    pub blobs: Vec<PackageBlob>,
    pub derived_fonts: Vec<FontDerivedSource>,
    pub external_font_families: Vec<String>,
    pub tool_locks: Vec<PackageToolLock>,
    pub plugin_policies: Vec<PluginPolicy>,
    pub plugin_bundles: Vec<PluginBundle>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackageInspection {
    pub manifest_sha256: String,
    pub source_project_id: String,
    pub source_revision: String,
    pub project_name: String,
    pub authoring_available: bool,
    pub mode: PackageMode,
    pub scope: PackageScope,
    pub revision_count: usize,
    pub transaction_count: usize,
    pub branch_count: usize,
    pub merge_count: usize,
    pub asset_version_count: usize,
    pub font_version_count: usize,
    pub derived_font_count: usize,
    pub plugin_bundle_count: usize,
    pub external_font_families: Vec<String>,
    pub fonts_missing: Vec<String>,
    pub tool_locks: Vec<PackageToolLock>,
    pub tool_environment_matches: bool,
    pub tool_lock_mismatches: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportReceipt {
    pub package_path: String,
    pub package: PackageInspection,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImportReceipt {
    pub target_project_id: String,
    pub revision: String,
    pub package: PackageInspection,
    /// Review and invoke separately. Import installs nothing and grants nothing.
    pub plugin_restore_plan: Vec<PluginGrantRestoreAction>,
}
