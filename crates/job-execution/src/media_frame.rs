//! Private persisted scene-frame preparation. No public native configuration.
use crate::scene_audio::TrustedJobAuthority;
use opencut_contracts::{AssetLockManifest, Time, scene::EditorSnapshot};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LocalVisualExecutionPolicy {
    pub max_frame_pixels: u64,
    pub max_decoded_pixels: u64,
    pub max_gpu_bytes: u64,
    pub max_rgba_bytes: u64,
    pub max_encoded_bytes: u64,
    pub max_output_bytes: u64,
    pub max_rss_bytes: u64,
    pub deadline_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreparedMediaFramePng {
    pub snapshot: EditorSnapshot,
    pub sequence_id: String,
    pub time: Time,
    pub asset_lock: AssetLockManifest,
    pub authority: TrustedJobAuthority,
    pub processing_policy: LocalVisualExecutionPolicy,
}
