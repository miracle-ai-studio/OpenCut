//! Owner-local project exchange package contract, version 1.
use opencut_contracts::ProjectDocument;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PackageMode {
    Linked,
    SelfContained,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportPackage {
    pub project_id: String,
    pub revision: String,
    /// Trusted owner-local destination directory, created only when absent.
    pub destination: String,
    pub mode: PackageMode,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImportPackage {
    /// Trusted owner-local package directory.
    pub package_path: String,
    pub target_project_id: String,
    pub expected_manifest_sha256: String,
    pub idempotency_key: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackageAsset {
    pub asset_id: String,
    pub content_hash: String,
    pub size: u64,
    /// Relative package path in self-contained mode; never supplied by clients.
    pub media_path: Option<String>,
    /// Origin hint in linked mode, verified byte-for-byte before import.
    pub linked_hint: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackageDependencies {
    /// Font families declared by caption styles. Font bytes and exact versions are not bundled.
    pub fonts_not_bundled: Vec<String>,
    /// Plugin package binaries remain external; schema 4 carries full project grant snapshots separately.
    pub plugins_not_bundled: Vec<String>,
    /// Required native tools without exact binary locks in this package.
    pub tools_not_locked: Vec<String>,
    pub unsupported: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackageManifest {
    pub schema_version: u32,
    pub mode: PackageMode,
    pub source_project_id: String,
    pub source_revision: String,
    pub project_path: String,
    pub project_sha256: String,
    pub assets: Vec<PackageAsset>,
    pub dependencies: PackageDependencies,
    pub self_contained_media: bool,
    pub fully_reproducible: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportReceipt {
    pub package_path: String,
    pub manifest_sha256: String,
    pub source_revision: String,
    pub asset_count: usize,
    pub dependencies: PackageDependencies,
    pub self_contained_media: bool,
    pub fully_reproducible: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreparedImport {
    pub manifest_sha256: String,
    pub manifest: PackageManifest,
    pub document: ProjectDocument,
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
    pub self_contained_media: bool,
    pub fully_reproducible: bool,
}
