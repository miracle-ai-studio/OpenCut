//! Durable unknown-outcome recovery for ephemeral previews. Secret control
//! credentials stay in the authenticated host slot, never in discovery output.
use crate::audio_preview_v4::{AudioPreviewControlV4, AudioPreviewCreateV4};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AudioPreviewRecoveryRequestV1 {
    Create {
        request: AudioPreviewCreateV4,
    },
    Control {
        create: AudioPreviewCreateV4,
        request: AudioPreviewControlV4,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StageAudioPreviewRecoveryV1 {
    pub request: AudioPreviewRecoveryRequestV1,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InspectAudioPreviewRecoveryV1 {
    pub project_id: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DispatchAudioPreviewRecoveryV1 {
    pub project_id: String,
    pub recovery_id: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AudioPreviewRecoveryDispositionV1 {
    Observed,
    Retire,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AckAudioPreviewRecoveryV1 {
    pub project_id: String,
    pub recovery_id: String,
    pub disposition: AudioPreviewRecoveryDispositionV1,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AudioPreviewRecoveryStatusV1 {
    Staged,
    Completed,
    Retired,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AudioPreviewRecoveryKindV1 {
    Create,
    Control,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioPreviewRecoveryRecordV1 {
    pub schema_version: u32,
    pub recovery_id: String,
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub request_digest: String,
    pub kind: AudioPreviewRecoveryKindV1,
    pub status: AudioPreviewRecoveryStatusV1,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioPreviewRecoveryInspectionV1 {
    pub schema_version: u32,
    pub record: Option<AudioPreviewRecoveryRecordV1>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioPreviewRecoveryAckV1 {
    pub schema_version: u32,
    pub recovery_id: String,
}
