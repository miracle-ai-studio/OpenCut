//! Versioned pinned-source font import. Caller input never grants CAS or URL access.
use crate::scene::FontStyle;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImportFont {
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
pub struct FontCatalogEntry {
    pub id: String,
    pub family: String,
    pub weight: u16,
    pub style: FontStyle,
    pub source_url: String,
    pub source_commit: String,
    pub content_hash: String,
    pub size_bytes: String,
    pub license_id: String,
    pub license_notice: String,
}
