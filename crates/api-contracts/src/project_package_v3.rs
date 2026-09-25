//! Schema 3 project packages with explicit native-tool locks and legacy schema 2 reads.
use crate::project_package::{PackageAsset, PackageDependencies, PackageMode};
use opencut_contracts::project_v3::ProjectDocumentV3;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportPackageV3 {
    pub project_id: String,
    pub revision: String,
    /// Owner-local new absolute directory.
    pub destination: String,
    pub mode: PackageMode,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImportPackageV3 {
    /// Owner-local canonical package directory.
    pub package_path: String,
    pub target_project_id: String,
    pub expected_manifest_sha256: String,
    pub idempotency_key: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackageManifestV3 {
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
    pub self_contained_media: bool,
    pub fully_reproducible: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreparedImportV3 {
    pub manifest_sha256: String,
    pub manifest: PackageManifestV3,
    pub document: ProjectDocumentV3,
    /// Whether target-host family names were checked during native package inspection.
    pub font_environment_checked: bool,
    pub fonts_missing: Vec<String>,
    /// Target-host result. This is computed during native package inspection.
    pub tool_environment_matches: bool,
    pub tool_lock_mismatches: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackageToolLock {
    pub name: String,
    pub version: String,
    pub binary_sha256: String,
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
    pub self_contained_media: bool,
    pub fully_reproducible: bool,
}
