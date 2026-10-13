//! Additive original-caption write recovery, without changing frozen V2 intent.
use crate::subtitle_v4::{CaptionReceiptV4, GenerateCaptionsV4, SubtitleImportV4};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EditRecoveryIntentV3 {
    SubtitleGenerateV4 { request: GenerateCaptionsV4 },
    SubtitleImportV4 { request: SubtitleImportV4 },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditRecoveryContextV3 {
    pub intent: EditRecoveryIntentV3,
    pub sequence_id: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StageEditRecoveryV3 {
    pub context: EditRecoveryContextV3,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadEditRecoveryV3 {
    pub project_id: String,
    pub recovery_id: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AckEditRecoveryV3 {
    pub project_id: String,
    pub recovery_id: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditRecoveryRecordV3 {
    pub schema_version: u32,
    pub recovery_id: String,
    pub project_id: String,
    pub request_digest: String,
    pub context: EditRecoveryContextV3,
    pub receipt: Option<CaptionReceiptV4>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditRecoveryInspectionV3 {
    pub schema_version: u32,
    pub record: Option<EditRecoveryRecordV3>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EditRecoveryResolutionV3 {
    Saved,
    NotSaved,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditRecoveryAckV3 {
    pub schema_version: u32,
    pub recovery_id: String,
    pub resolution: EditRecoveryResolutionV3,
    pub receipt: Option<CaptionReceiptV4>,
}
