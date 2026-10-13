//! Durable recovery of the exact pinned-source font import, independent of
//! frozen V1/V2 edit recovery intents. Scope is supplied by the trusted host.
use crate::{TransactionResult, font_import_v1::ImportFontV1};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontRecoveryContextV1 {
    pub request: ImportFontV1,
    /// UI location only; never changes the executable import request.
    pub sequence_id: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StageFontRecoveryV1 {
    pub context: FontRecoveryContextV1,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadFontRecoveryV1 {
    pub project_id: String,
    pub recovery_id: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AckFontRecoveryV1 {
    pub project_id: String,
    pub recovery_id: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontRecoveryRecordV1 {
    pub schema_version: u32,
    pub recovery_id: String,
    pub project_id: String,
    pub request_digest: String,
    pub context: FontRecoveryContextV1,
    pub receipt: Option<TransactionResult>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontRecoveryInspectionV1 {
    pub schema_version: u32,
    pub record: Option<FontRecoveryRecordV1>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FontRecoveryResolutionV1 {
    Saved,
    NotSaved,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontRecoveryAckV1 {
    pub schema_version: u32,
    pub recovery_id: String,
    pub resolution: FontRecoveryResolutionV1,
    pub receipt: Option<TransactionResult>,
}
