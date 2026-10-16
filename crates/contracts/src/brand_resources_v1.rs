//! Pinned, offline brand catalog. Collection licensing never substitutes for an icon license.
use crate::{catalog_resources_v1::CatalogUploadedMediaV1, project_v4::ResourceLicenseV4};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListBrandResourcesV1 {
    pub query: Option<String>,
    pub offset: u32,
    pub limit: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrandCollectionV1 {
    pub name: String,
    pub release: String,
    pub commit: String,
    pub source_url: String,
    pub inventory_sha256: String,
    pub collection_license: ResourceLicenseV4,
    pub disclaimer: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrandCatalogEntryV1 {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub keywords: Vec<String>,
    pub color: String,
    pub source_url: String,
    pub guidelines_url: Option<String>,
    pub svg_sha256: String,
    pub svg_size: u64,
    /// The icon's own recorded license, never the collection's license.
    pub license_identifier: Option<String>,
    pub license_url: Option<String>,
    pub import_available: bool,
    pub unavailable_reason: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrandCatalogV1 {
    pub schema_version: u32,
    pub collection: BrandCollectionV1,
    pub total: u32,
    pub matched: u32,
    pub offset: u32,
    pub next_offset: Option<u32>,
    pub entries: Vec<BrandCatalogEntryV1>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GetBrandSourceV1 {
    pub catalog_id: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrandSourceV1 {
    pub schema_version: u32,
    pub collection: BrandCollectionV1,
    pub entry: BrandCatalogEntryV1,
    pub svg: String,
    pub license: ResourceLicenseV4,
    pub raster_width: u32,
    pub raster_height: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImportBrandResourceV1 {
    pub schema_version: u32,
    pub project_id: String,
    pub expected_revision: String,
    pub idempotency_key: String,
    pub catalog_id: String,
    pub asset_id: String,
    pub uploaded: CatalogUploadedMediaV1,
}
