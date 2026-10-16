//! Self-contained, fixed-revision V4 exchange. Package schemas 1-5 stay frozen.
use opencut_contracts::font_derived_v1::FontDerivedSourceV1;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportPackageV4 {
    pub project_id: String,
    pub revision: String,
    /// Owner-local new absolute directory. No existing destination is replaced.
    pub destination: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImportPackageV4 {
    pub package_path: String,
    pub target_project_id: String,
    pub expected_manifest_sha256: String,
    pub idempotency_key: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InspectPackageV4 {
    pub package_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackageInspectionV4 {
    pub manifest_sha256: String,
    pub source_project_id: String,
    pub source_revision: String,
    pub project_name: String,
    pub asset_count: usize,
    pub font_count: usize,
    pub derived_font_count: usize,
    pub self_contained_resources: bool,
    pub history_included: bool,
    pub fully_reproducible: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackageResourceV4 {
    pub resource_id: String,
    pub content_hash: String,
    pub size_bytes: u64,
    /// Canonical package-relative media/<hash> or fonts/<hash>.
    pub path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackageManifestV4 {
    pub schema_version: u32,
    pub project_schema_version: u32,
    pub source_project_id: String,
    pub source_revision: String,
    pub project_path: String,
    pub project_sha256: String,
    pub assets: Vec<PackageResourceV4>,
    pub fonts: Vec<PackageResourceV4>,
    pub derived_fonts: Vec<FontDerivedSourceV1>,
    pub self_contained_resources: bool,
    pub history_included: bool,
    /// Bundling bytes is not a renderer reproducibility certificate.
    pub fully_reproducible: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportReceiptV4 {
    pub package_path: String,
    pub manifest_sha256: String,
    pub source_revision: String,
    pub asset_count: usize,
    pub font_count: usize,
    pub derived_font_count: usize,
    pub self_contained_resources: bool,
    pub history_included: bool,
    pub fully_reproducible: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImportReceiptV4 {
    pub target_project_id: String,
    pub revision: String,
    pub manifest_sha256: String,
    pub source_project_id: String,
    pub source_revision: String,
    pub asset_count: usize,
    pub font_count: usize,
    pub derived_font_count: usize,
    pub self_contained_resources: bool,
    pub history_included: bool,
    pub fully_reproducible: bool,
}
