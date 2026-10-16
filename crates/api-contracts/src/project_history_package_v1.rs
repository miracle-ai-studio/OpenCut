//! Additive schema 7: complete V4 revision history, without external authority.
use opencut_contracts::font_derived_v1::FontDerivedSourceV1;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportHistoryPackageV1 {
    pub project_id: String,
    /// Main head must still match when the complete branch snapshot is captured.
    pub expected_revision: String,
    pub destination: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InspectHistoryPackageV1 {
    pub package_path: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImportHistoryPackageV1 {
    pub package_path: String,
    pub target_project_id: String,
    pub expected_manifest_sha256: String,
    pub idempotency_key: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoryPackageBlobV1 {
    pub content_hash: String,
    pub size_bytes: u64,
    /// Exactly blobs/<sha256>; shared by all historical media/font versions.
    pub path: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoryPackageManifestV1 {
    pub schema_version: u32,
    pub project_schema_version: u32,
    pub source_project_id: String,
    pub source_revision: String,
    pub history_path: String,
    pub history_sha256: String,
    pub revision_count: usize,
    pub transaction_count: usize,
    /// Explicit non-main branches; the main head is source_revision.
    pub branch_count: usize,
    pub merge_count: usize,
    pub asset_version_count: usize,
    pub font_version_count: usize,
    pub blobs: Vec<HistoryPackageBlobV1>,
    pub derived_fonts: Vec<FontDerivedSourceV1>,
    pub self_contained_resources: bool,
    pub history_included: bool,
    pub fully_reproducible: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoryPackageInspectionV1 {
    pub manifest_sha256: String,
    pub source_project_id: String,
    pub source_revision: String,
    pub project_name: String,
    pub revision_count: usize,
    pub transaction_count: usize,
    pub branch_count: usize,
    pub merge_count: usize,
    pub asset_version_count: usize,
    pub font_version_count: usize,
    pub derived_font_count: usize,
    pub self_contained_resources: bool,
    pub history_included: bool,
    pub fully_reproducible: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportHistoryPackageReceiptV1 {
    pub package_path: String,
    pub package: HistoryPackageInspectionV1,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImportHistoryPackageReceiptV1 {
    pub target_project_id: String,
    pub revision: String,
    pub package: HistoryPackageInspectionV1,
}
