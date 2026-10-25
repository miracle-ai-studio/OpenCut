//! Direct Classic source conversion. Resource selectors name authorized fixed
//! revisions; prepared documents and native resource locators stay internal.
use opencut_contracts::{
    project_public::PublicProjectDocument,
    scene::{FontStyle, FontWeight},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AnalyzeClassic {
    /// Original SerializedProject JSON, never an old converted document.
    pub content: String,
    pub target_project_id: String,
    /// All mapped library assets, including unused media, are preserved.
    pub media_mappings: Vec<ClassicMediaMapping>,
    #[serde(default)]
    pub font_mappings: Vec<ClassicFontMapping>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClassicFontMapping {
    pub classic_family: String,
    pub font_weight: FontWeight,
    pub font_style: FontStyle,
    pub source_project_id: String,
    pub source_revision: String,
    pub font_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClassicFontRequest {
    pub family: String,
    pub font_weight: FontWeight,
    pub font_style: FontStyle,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClassicReview {
    pub source_format: String,
    pub source_version: Option<u32>,
    pub source_content_id: String,
    pub target_project_id: String,
    pub mapped: Vec<String>,
    pub losses: Vec<CompatibilityFinding>,
    pub blockers: Vec<CompatibilityFinding>,
    /// Preserved with strict font resolution. Saving does not establish rendering
    /// availability and never triggers font discovery or network downloads.
    pub unresolved_fonts: Vec<ClassicFontRequest>,
    /// Present only when all editing-relevant semantics have explicit mappings.
    pub proposed_project: Option<PublicProjectDocument>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImportClassic {
    pub source: AnalyzeClassic,
    pub expected_source_content_id: String,
    pub idempotency_key: String,
    pub accepted_loss_codes: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClassicRestoreReceipt {
    pub target_project_id: String,
    pub revision: String,
    pub source_content_id: String,
    pub asset_count: usize,
    pub font_count: usize,
    pub unresolved_fonts: Vec<ClassicFontRequest>,
    pub losses: Vec<CompatibilityFinding>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClassicMediaMapping {
    pub classic_media_id: String,
    pub source_project_id: String,
    pub source_revision: String,
    pub asset_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompatibilityFinding {
    pub path: String,
    pub code: String,
    pub detail: String,
}
