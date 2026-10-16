//! Fixed resource selection. A catalog ID is never a network locator or a CAS grant.
use crate::{AssetVersion, project_v4::ResourceLicenseV4};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CatalogResourceCategoryV1 {
    Shapes,
    Flags,
    Sounds,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CatalogResourceEntryV1 {
    pub id: String,
    pub name: String,
    pub category: CatalogResourceCategoryV1,
    pub keywords: Vec<String>,
    pub source_url: String,
    pub source_sha256: Option<String>,
    pub license: ResourceLicenseV4,
    pub width: Option<u32>,
    pub height: Option<u32>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResourceCatalogV1 {
    pub schema_version: u32,
    pub sound_download_available: bool,
    pub sound_unavailable_reason: Option<String>,
    pub entries: Vec<CatalogResourceEntryV1>,
    /// Classic's provider is empty; no unavailable brand is advertised as selectable.
    pub brands_available: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CatalogUploadedMediaV1 {
    pub media_id: String,
    pub version: AssetVersion,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImportCatalogResourceV1 {
    pub project_id: String,
    pub expected_revision: String,
    pub idempotency_key: String,
    pub catalog_id: String,
    pub asset_id: String,
    /// Flags are browser-rasterized PNGs through ordinary authorized uploads.
    /// Sounds have no uploaded selector: the native fixed-source port supplies bytes.
    pub uploaded: Option<CatalogUploadedMediaV1>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LocalSoundKindV1 {
    Whoosh,
    Click,
    Impact,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LocalSoundRecipeV1 {
    pub kind: LocalSoundKindV1,
    pub duration_ms: u32,
    pub seed: u32,
    pub level_db: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GenerateCatalogSoundV1 {
    pub project_id: String,
    pub expected_revision: String,
    pub idempotency_key: String,
    pub asset_id: String,
    pub recipe: LocalSoundRecipeV1,
}
