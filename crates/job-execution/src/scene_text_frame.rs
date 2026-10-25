//! Private fixed-revision complete frame preparation. No paths enter public DTOs.
use crate::{media_frame::LocalVisualExecutionPolicy, scene_audio::TrustedJobAuthority};
use opencut_contracts::{AssetLockManifest, Time, scene::EditorSnapshot, text_frame::FontLock};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreparedTextFramePng {
    pub snapshot: EditorSnapshot,
    pub sequence_id: String,
    pub time: Time,
    pub asset_lock: AssetLockManifest,
    pub font_lock: Vec<FontLock>,
    pub authority: TrustedJobAuthority,
    pub processing_policy: LocalVisualExecutionPolicy,
}
