//! Additive schema 3 project packages; the schema 2 package contract is frozen.
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
    pub self_contained_media: bool,
    pub fully_reproducible: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreparedImportV3 {
    pub manifest_sha256: String,
    pub manifest: PackageManifestV3,
    pub document: ProjectDocumentV3,
}

pub use crate::project_package::{ExportReceipt, ImportReceipt};
