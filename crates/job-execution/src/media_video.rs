//! Trusted private preparation; native paths never cross the public boundary.
use crate::media_frame::LocalVisualExecutionPolicy;
use crate::scene_audio::{LocalAudioExecutionPolicy, TrustedJobAuthority};
use opencut_contracts::{AssetLockManifest, scene::EditorSnapshot, text_frame::FontLock};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The private browser counter must represent every integer frame exactly.
pub const MAX_VIDEO_FRAMES: u64 = (1u64 << 53) - 1;
/// Node's native timeout accepts a positive signed32-bit millisecond delay.
pub const MAX_VIDEO_DEADLINE_MS: u64 = i32::MAX as u64;

/// Actual execution policy,not a renderer/API generation selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum VideoExecutionLimit {
    Total { milliseconds: u64 },
    Progress { milliseconds: u64 },
}
impl VideoExecutionLimit {
    pub fn milliseconds(self) -> u64 {
        match self {
            Self::Total { milliseconds } | Self::Progress { milliseconds } => milliseconds,
        }
    }
    pub fn renews_on_progress(self) -> bool {
        matches!(self, Self::Progress { .. })
    }
    pub fn remaining_after(self, elapsed: std::time::Duration) -> Self {
        match self {
            Self::Total { milliseconds } => Self::Total {
                milliseconds: milliseconds
                    .saturating_sub(u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)),
            },
            Self::Progress { .. } => self,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LocalVideoExecutionPolicy {
    pub visual: LocalVisualExecutionPolicy,
    pub audio: LocalAudioExecutionPolicy,
    /// Captured workload policy,distinct from decoded-frame residency.
    pub max_frames: u64,
    /// Captured byte ceiling; actual disk admission can be lower.
    pub max_output_bytes: u64,
    /// Exact private historical policy. New jobs omit this total-time field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline_ms: Option<u64>,
    /// New jobs expire only when admitted work stops advancing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stalled_after_ms: Option<u64>,
}
impl LocalVideoExecutionPolicy {
    pub fn execution_limit(&self) -> Option<VideoExecutionLimit> {
        let limit = match (self.deadline_ms, self.stalled_after_ms) {
            (Some(milliseconds), None) => VideoExecutionLimit::Total { milliseconds },
            (None, Some(milliseconds)) => VideoExecutionLimit::Progress { milliseconds },
            _ => return None,
        };
        (1..=MAX_VIDEO_DEADLINE_MS)
            .contains(&limit.milliseconds())
            .then_some(limit)
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreparedMediaVideoMp4 {
    pub snapshot: EditorSnapshot,
    pub sequence_id: String,
    pub asset_lock: AssetLockManifest,
    pub font_lock: Vec<FontLock>,
    pub authority: TrustedJobAuthority,
    pub processing_policy: LocalVideoExecutionPolicy,
}
