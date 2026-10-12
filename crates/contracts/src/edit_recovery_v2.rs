//! Exact fixed-head write recovery. Host-authenticated scope owns the record;
//! neither its locator nor a project revision grants permission to replay it.
use crate::{
    TransactionResult,
    edit_recovery_v1::EditRecoveryRecordV1,
    project_v4_history::{HistoryCommitV4, RedoV4Project, UndoV4Project},
    project_v4_ops::RestoreV4Revision,
    project_v4_public::ApplyPublicV4Project,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EditRecoveryIntentV2 {
    PublicApply { request: Box<ApplyPublicV4Project> },
    Undo { request: UndoV4Project },
    Redo { request: RedoV4Project },
    Restore { request: RestoreV4Revision },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditRecoveryContextV2 {
    pub intent: EditRecoveryIntentV2,
    /// Optional UI location only. History and restore remain project-scoped;
    /// this field does not select or transform the executable request.
    pub sequence_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StageEditRecoveryV2 {
    pub context: EditRecoveryContextV2,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadEditRecoveryV2 {
    pub project_id: String,
    pub recovery_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AckEditRecoveryV2 {
    pub project_id: String,
    pub recovery_id: String,
}

/// Preserve the original public reply, including selective-history identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EditRecoveryReceiptV2 {
    PublicApply { transaction: TransactionResult },
    Undo { commit: HistoryCommitV4 },
    Redo { commit: HistoryCommitV4 },
    Restore { transaction: TransactionResult },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditRecoveryRecordV2 {
    pub schema_version: u32,
    pub recovery_id: String,
    pub project_id: String,
    /// Digest of the exact staged context. It is not a renderer identity.
    pub request_digest: String,
    pub context: EditRecoveryContextV2,
    pub receipt: Option<EditRecoveryReceiptV2>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DiscoveredEditRecoveryV2 {
    LegacyV1 { record: Box<EditRecoveryRecordV1> },
    CurrentV2 { record: Box<EditRecoveryRecordV2> },
}

/// Absence is a field value, never a missing success payload. Discovery checks
/// existing V1 records first without rewriting their frozen context or digest.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditRecoveryInspectionV2 {
    pub schema_version: u32,
    pub record: Option<DiscoveredEditRecoveryV2>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EditRecoveryResolutionV2 {
    Saved,
    NotSaved,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditRecoveryAckV2 {
    pub schema_version: u32,
    pub recovery_id: String,
    pub resolution: EditRecoveryResolutionV2,
    pub receipt: Option<EditRecoveryReceiptV2>,
}
