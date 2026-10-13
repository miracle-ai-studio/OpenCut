//! Versioned pinned-source font import. Caller input never grants CAS or URL access.
use crate::project_v4::FontStyleV4;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImportFontV1 {
    pub project_id: String,
    pub expected_revision: String,
    pub idempotency_key: String,
    pub catalog_id: String,
    pub font_id: String,
    /// Checked against these exact bytes; rendering rechecks its current text.
    pub required_text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontCatalogEntryV1 {
    pub id: String,
    pub family: String,
    pub weight: u16,
    pub style: FontStyleV4,
    pub source_url: String,
    pub source_commit: String,
    pub content_hash: String,
    pub size_bytes: String,
    pub license_id: String,
    pub license_notice: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontCatalogV1 {
    pub schema_version: u32,
    pub download_available: bool,
    pub unavailable_reason: Option<String>,
    pub fonts: Vec<FontCatalogEntryV1>,
}
