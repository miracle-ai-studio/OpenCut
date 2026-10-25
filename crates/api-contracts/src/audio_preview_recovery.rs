//! Durable unknown-outcome recovery for ephemeral previews. Secret control
//! credentials stay in the authenticated host slot, never in discovery output.
use crate::audio_preview::{AudioPreviewControl, AudioPreviewCreate};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AudioPreviewRecoveryRequest {
    Create {
        request: AudioPreviewCreate,
    },
    Control {
        create: AudioPreviewCreate,
        request: AudioPreviewControl,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StageAudioPreviewRecovery {
    pub request: AudioPreviewRecoveryRequest,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InspectAudioPreviewRecovery {
    pub project_id: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DispatchAudioPreviewRecovery {
    pub project_id: String,
    pub recovery_id: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AudioPreviewRecoveryDisposition {
    Observed,
    Retire,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AckAudioPreviewRecovery {
    pub project_id: String,
    pub recovery_id: String,
    pub disposition: AudioPreviewRecoveryDisposition,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AudioPreviewRecoveryStatus {
    Staged,
    Completed,
    Retired,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AudioPreviewRecoveryKind {
    Create,
    Control,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioPreviewRecoveryRecord {
    pub recovery_id: String,
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub request_digest: String,
    pub kind: AudioPreviewRecoveryKind,
    pub status: AudioPreviewRecoveryStatus,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioPreviewRecoveryInspection {
    pub record: Option<AudioPreviewRecoveryRecord>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioPreviewRecoveryAck {
    pub recovery_id: String,
}
