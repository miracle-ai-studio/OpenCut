//! Pinned variable-font recipes. Public input selects a reviewed recipe, never
//! axes, URLs, local runtimes or an asserted derivation proof.
use crate::{TransactionResult, scene::FontStyle};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontDerivedFace {
    pub id: String,
    pub family: String,
    pub weight: u16,
    pub style: FontStyle,
    pub derivation_id: String,
    pub content_hash: String,
    pub size_bytes: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontDerivedAxis {
    pub tag: String,
    /// Exact decimal coordinate retained from the canonical derivation record.
    pub coordinate: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontDerivedSource {
    pub face: FontDerivedFace,
    pub source_commit: String,
    pub source_url: String,
    pub source_sha256: String,
    pub source_size_bytes: String,
    pub license_url: String,
    pub license_id: String,
    pub license_notice: String,
    pub axes: Vec<FontDerivedAxis>,
    /// Original canonical UTF-8 derivation record, including source/license/tool
    /// identity and all axes. Hash its object without derivation_id to verify ID.
    pub proof_json: String,
}

pub use crate::font_catalog::FontSourceRequest;

pub use crate::font_import::ImportFont;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadDerivedFontProof {
    pub project_id: String,
    pub revision: String,
    pub font_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DerivedFontProof {
    pub project_id: String,
    pub revision: String,
    pub font_id: String,
    pub source: FontDerivedSource,
}
