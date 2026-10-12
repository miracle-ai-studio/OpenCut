//! Fixed-head source-audio planning. Public apply owns atomic publication and
//! its receipt; a read-only plan has no separate mutation receipt.
use crate::{TimeRange, project_v4_public::ApplyPublicV4Project};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceAudioRequestV4 {
    pub project_id: String,
    pub expected_revision: String,
    pub sequence_id: String,
    pub track_id: String,
    pub element_id: String,
    pub idempotency_key: String,
    pub label: String,
    pub command: SourceAudioCommandV4,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SourceAudioCommandV4 {
    Separate {
        audio_element_id: String,
        destination: SourceAudioDestinationV4,
    },
    /// Re-enable the original video. Existing detached audio remains untouched;
    /// restoring audio is not selective undo and can intentionally mix both.
    Restore,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SourceAudioDestinationV4 {
    Existing {
        track_id: String,
    },
    /// Choose the first unlocked, non-overlapping audio track in scene order.
    /// If none fits, append an unmuted default "Audio track" with this stable ID.
    FirstAvailable {
        fallback_track_id: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceAudioPlanV4 {
    pub transaction: ApplyPublicV4Project,
    /// Identity of the original planning input, not a committed import receipt.
    pub original_request_digest: String,
    pub decision: SourceAudioDecisionV4,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceAudioDecisionV4 {
    pub source_element_id: String,
    pub asset_id: String,
    pub source_audio_enabled: bool,
    /// Nominal request clock from the existing element, never a decoder window.
    pub source_request: TimeRange,
    pub detached: Option<DetachedAudioDecisionV4>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DetachedAudioDecisionV4 {
    pub element_id: String,
    pub track_id: String,
    pub track_created: bool,
    pub copied_channels: Vec<SourceAudioChannelCopyV4>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceAudioChannelCopyV4 {
    pub source_channel_id: String,
    pub channel_id: String,
    pub key_ids: Vec<SourceAudioIdentityCopyV4>,
    pub segment_ids: Vec<SourceAudioIdentityCopyV4>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceAudioIdentityCopyV4 {
    pub source_id: String,
    pub id: String,
}
