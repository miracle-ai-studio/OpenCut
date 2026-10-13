//! Lightweight discovery of pinned Classic faces. Source details and license
//! notices are fetched for one selected face; metadata never grants resource access.
use crate::project_v4::FontStyleV4;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontBrowserCatalogV2 {
    pub schema_version: u32,
    pub download_available: bool,
    pub unavailable_reason: Option<String>,
    pub source_commit: String,
    pub classic_atlas_sha256: String,
    pub classic_family_count: u32,
    pub pinned_face_count: u32,
    pub families: Vec<FontBrowserFamilyV2>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontBrowserFamilyV2 {
    pub family: String,
    pub category: FontBrowserCategoryV2,
    pub faces: Vec<FontBrowserFaceV2>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FontBrowserCategoryV2 {
    Google,
    System,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontBrowserFaceV2 {
    pub weight: u16,
    pub style: FontStyleV4,
    pub catalog_id: Option<String>,
    pub unavailable_reason: Option<FontBrowserUnavailableReasonV2>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FontBrowserUnavailableReasonV2 {
    WeightNotExpressedByTextV4,
    NoPinnedStaticSource,
    SystemFontNotRedistributable,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontSourceRequestV2 {
    pub catalog_id: String,
}
