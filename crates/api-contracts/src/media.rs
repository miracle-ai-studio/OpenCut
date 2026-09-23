pub use opencut_contracts::CatalogEntry;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InspectMediaInput {
    pub content_hash: String,
}
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RepairMediaInput {
    pub content_hash: String,
    pub expected_metadata_hash: Option<String>,
}
