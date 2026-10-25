//! Schema 3 project packages with explicit native-tool locks and legacy schema 2 reads.
use crate::project_package::{PackageAsset, PackageDependencies, PackageMode};
use opencut_contracts::historical_visual::HistoricalVisualDocument;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoricalVisualExport {
    pub project_id: String,
    pub revision: String,
    /// Owner-local new absolute directory.
    pub destination: String,
    pub mode: PackageMode,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoricalVisualImport {
    /// Owner-local canonical package directory.
    pub package_path: String,
    pub target_project_id: String,
    pub expected_manifest_sha256: String,
    pub idempotency_key: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoricalVisualManifest {
    pub schema_version: u32,
    pub project_schema_version: u32,
    pub mode: PackageMode,
    pub source_project_id: String,
    pub source_revision: String,
    pub project_path: String,
    pub project_sha256: String,
    pub assets: Vec<PackageAsset>,
    pub dependencies: PackageDependencies,
    /// Exact FFmpeg tool identities. Empty only in legacy schema 2 manifests.
    #[serde(default)]
    pub tool_locks: Vec<PackageToolLock>,
    /// Exact project grant policy snapshots. Added in package schema 4; old package manifests omit it.
    #[serde(default)]
    pub plugin_grants: Vec<PackagePluginGrant>,
    /// Exact installed plugin package identities and relative content roots. Added in schema 5.
    #[serde(default)]
    pub plugin_bundles: Vec<PackagePluginBundle>,
    pub self_contained_media: bool,
    pub fully_reproducible: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackagePluginBundle {
    pub kind: PackagePluginKind,
    pub id: String,
    pub version: String,
    pub package_sha256: String,
    /// Source V1 permission snapshot. It is a reviewed install proposal, never applied on import.
    #[serde(default)]
    pub proposed_global_permissions: BTreeSet<String>,
    /// Canonical package-relative directory under `plugins/`.
    pub path: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PackagePluginKind {
    PlannerV1,
    PresetV2,
}

/// Project authorization policy is carried for review and restoration, never applied on import.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PackagePluginGrant {
    PlannerV1 {
        id: String,
        version: String,
        package_sha256: String,
        operations: BTreeSet<String>,
        enabled: bool,
    },
    PresetV2 {
        id: String,
        version: String,
        package_sha256: String,
        preset_names: BTreeSet<String>,
        enabled: bool,
    },
}

pub use crate::project_exchange::{PackageToolLock, PluginGrantRestoreAction};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreparedHistoricalVisual {
    pub manifest_sha256: String,
    pub manifest: HistoricalVisualManifest,
    pub document: HistoricalVisualDocument,
    /// Whether target-host family names were checked during native package inspection.
    pub font_environment_checked: bool,
    pub fonts_missing: Vec<String>,
    /// Target-host result. This is computed during native package inspection.
    pub tool_environment_matches: bool,
    pub tool_lock_mismatches: Vec<String>,
    /// Verified direct install calls for bundles; never applied by package import.
    #[serde(default)]
    pub plugin_package_install_plan: Vec<PluginGrantRestoreAction>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportReceipt {
    pub package_path: String,
    pub manifest_sha256: String,
    pub source_revision: String,
    pub asset_count: usize,
    pub dependencies: PackageDependencies,
    pub tool_locks: Vec<PackageToolLock>,
    /// Grant snapshots written into the package, never applied during export.
    pub plugin_grants: Vec<PackagePluginGrant>,
    pub self_contained_media: bool,
    pub fully_reproducible: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImportReceipt {
    pub target_project_id: String,
    pub revision: String,
    pub manifest_sha256: String,
    pub source_project_id: String,
    pub source_revision: String,
    pub dependencies: PackageDependencies,
    pub font_environment_checked: bool,
    pub fonts_missing: Vec<String>,
    pub tool_locks: Vec<PackageToolLock>,
    pub tool_environment_matches: bool,
    pub tool_lock_mismatches: Vec<String>,
    /// Direct install calls followed by grant calls for Owner review; import executes neither.
    #[serde(default)]
    pub plugin_grant_restore_plan: Vec<PluginGrantRestoreAction>,
    pub self_contained_media: bool,
    pub fully_reproducible: bool,
}
