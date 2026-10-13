//! Pinned variable-font recipes. Public input selects a reviewed recipe, never
//! axes, URLs, local runtimes or an asserted derivation proof.
use crate::{TransactionResult, project_v4::FontStyleV4};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontDerivedFaceV1 {
    pub id: String,
    pub family: String,
    pub weight: u16,
    pub style: FontStyleV4,
    pub derivation_id: String,
    pub content_hash: String,
    pub size_bytes: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontDerivedCatalogV1 {
    pub schema_version: u32,
    pub import_available: bool,
    pub unavailable_reason: Option<String>,
    pub source_commit: String,
    pub fonts: Vec<FontDerivedFaceV1>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontDerivedAxisV1 {
    pub tag: String,
    /// Exact decimal coordinate retained from the canonical derivation record.
    pub coordinate: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontDerivedSourceV1 {
    pub face: FontDerivedFaceV1,
    pub source_commit: String,
    pub source_url: String,
    pub source_sha256: String,
    pub source_size_bytes: String,
    pub license_url: String,
    pub license_id: String,
    pub license_notice: String,
    pub axes: Vec<FontDerivedAxisV1>,
    /// Original canonical UTF-8 derivation record, including source/license/tool
    /// identity and all axes. Hash its object without derivation_id to verify ID.
    pub proof_json: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontDerivedSourceRequestV1 {
    pub catalog_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImportDerivedFontV1 {
    pub project_id: String,
    pub expected_revision: String,
    pub idempotency_key: String,
    pub catalog_id: String,
    pub font_id: String,
    pub required_text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadDerivedFontProofV1 {
    pub project_id: String,
    pub revision: String,
    pub font_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DerivedFontProofV1 {
    pub schema_version: u32,
    pub project_id: String,
    pub revision: String,
    pub font_id: String,
    pub source: FontDerivedSourceV1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DerivedFontRecoveryContextV1 {
    pub request: ImportDerivedFontV1,
    pub sequence_id: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StageDerivedFontRecoveryV1 {
    pub context: DerivedFontRecoveryContextV1,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadDerivedFontRecoveryV1 {
    pub project_id: String,
    pub recovery_id: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AckDerivedFontRecoveryV1 {
    pub project_id: String,
    pub recovery_id: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DerivedFontRecoveryRecordV1 {
    pub schema_version: u32,
    pub recovery_id: String,
    pub project_id: String,
    pub request_digest: String,
    pub context: DerivedFontRecoveryContextV1,
    pub receipt: Option<TransactionResult>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DerivedFontRecoveryInspectionV1 {
    pub schema_version: u32,
    pub record: Option<DerivedFontRecoveryRecordV1>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DerivedFontRecoveryAckV1 {
    pub schema_version: u32,
    pub recovery_id: String,
    pub resolution: crate::font_recovery_v1::FontRecoveryResolutionV1,
    pub receipt: Option<TransactionResult>,
}
