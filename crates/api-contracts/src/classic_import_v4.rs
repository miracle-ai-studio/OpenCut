//! Direct Classic source conversion. Resource selectors name authorized fixed
//! revisions; prepared documents and native resource locators stay internal.
use crate::classic_import::{ClassicMediaMapping, CompatibilityFinding};
use opencut_contracts::{
    project_v4::{FontStyleV4, FontWeightV4},
    project_v4_public::PublicProjectDocumentV4,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClassicAnalyzeV4 {
    /// Original SerializedProject JSON, never an old converted document.
    pub content: String,
    pub target_project_id: String,
    /// All mapped library assets, including unused media, are preserved.
    pub media_mappings: Vec<ClassicMediaMapping>,
    pub font_mappings: Vec<ClassicFontMappingV4>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClassicFontMappingV4 {
    pub classic_family: String,
    pub font_weight: FontWeightV4,
    pub font_style: FontStyleV4,
    pub source_project_id: String,
    pub source_revision: String,
    pub font_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClassicFontRequestV4 {
    pub family: String,
    pub font_weight: FontWeightV4,
    pub font_style: FontStyleV4,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClassicAnalysisV4 {
    pub schema_version: u32,
    pub source_format: String,
    pub source_version: Option<u32>,
    pub source_content_id: String,
    pub target_project_id: String,
    pub mapped: Vec<String>,
    pub losses: Vec<CompatibilityFinding>,
    pub blockers: Vec<CompatibilityFinding>,
    /// Preserved with strict font resolution. Saving does not establish rendering
    /// availability and never triggers font discovery or network downloads.
    pub unresolved_fonts: Vec<ClassicFontRequestV4>,
    /// Present only when all editing-relevant semantics have explicit mappings.
    pub proposed_project: Option<PublicProjectDocumentV4>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClassicImportV4 {
    pub source: ClassicAnalyzeV4,
    pub expected_source_content_id: String,
    pub idempotency_key: String,
    pub accepted_loss_codes: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClassicImportReceiptV4 {
    pub schema_version: u32,
    pub target_project_id: String,
    pub revision: String,
    pub source_content_id: String,
    pub asset_count: usize,
    pub font_count: usize,
    pub unresolved_fonts: Vec<ClassicFontRequestV4>,
    pub losses: Vec<CompatibilityFinding>,
}
