//! Read-only authored gain planning. The returned public edit owns its single
//! commit receipt; planning never changes a revision or captures media bytes.
use crate::{project_v4::GainV4, project_v4_public::ApplyPublicV4Project};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioGainRequestV4 {
    pub project_id: String,
    pub expected_revision: String,
    pub sequence_id: String,
    pub track_id: String,
    pub element_id: String,
    pub idempotency_key: String,
    pub label: String,
    pub command: AudioGainCommandV4,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AudioGainCommandV4 {
    /// Classic clip-line input, -600..200 inclusive in tenths of a decibel.
    /// This does not quantize existing stored gain or animation key values.
    SetDecibels { decibels_tenths: i32 },
    /// Muting is independent of gain and never removes existing automation.
    SetMuted { muted: bool },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioGainPlanV4 {
    /// Digest of the original plan request, not a mutation receipt.
    pub original_request_digest: String,
    pub decision: AudioGainDecisionV4,
    /// No authored change means no revision or idempotency receipt is written.
    pub transaction: Option<ApplyPublicV4Project>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioGainDecisionV4 {
    pub element_id: String,
    pub old_gain: GainV4,
    pub new_gain: GainV4,
    pub old_element_muted: bool,
    pub element_muted: bool,
    pub track_muted: bool,
    /// Any nonempty element gain channel hides the Classic static clip line.
    /// Muting remains available; this planner never edits or deletes its keys.
    pub animated: bool,
}
