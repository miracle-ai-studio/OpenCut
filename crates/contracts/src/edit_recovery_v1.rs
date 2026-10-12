//! Bounded recovery of one exact reviewed transaction. Identity comes from the
//! authenticated host; neither a tab ID nor a recovery ID grants access.
use crate::{
    TransactionResult,
    timeline_v4::{TimelineEditPlanV4, TimelineEditRequestV4},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EditRecoveryContextV1 {
    TimelineV4 {
        request: TimelineEditRequestV4,
        plan: TimelineEditPlanV4,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StageEditRecoveryV1 {
    pub context: EditRecoveryContextV1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadEditRecoveryV1 {
    pub project_id: String,
    /// None discovers the current authenticated scope's sole unresolved record
    /// for this project, including stage-success/browser-storage-failure cases.
    pub recovery_id: Option<String>,
}

/// A non-null success payload keeps absence distinct from a missing protocol
/// envelope. The nullable legacy read declaration remains frozen.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditRecoveryInspectionV1 {
    pub schema_version: u32,
    pub record: Option<EditRecoveryRecordV1>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AckEditRecoveryV1 {
    pub project_id: String,
    pub recovery_id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditRecoveryRecordV1 {
    pub schema_version: u32,
    pub recovery_id: String,
    pub project_id: String,
    pub sequence_id: String,
    /// Identity of the exact typed context, not a renderer identity or grant.
    pub request_digest: String,
    pub context: EditRecoveryContextV1,
    /// Only the authoritative original apply receipt can populate this field.
    pub receipt: Option<TransactionResult>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EditRecoveryResolutionV1 {
    Saved,
    NotSaved,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditRecoveryAckV1 {
    pub schema_version: u32,
    pub recovery_id: String,
    pub resolution: EditRecoveryResolutionV1,
    pub receipt: Option<TransactionResult>,
}
