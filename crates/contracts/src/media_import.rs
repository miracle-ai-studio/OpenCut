//! Native V4 media intent. Selectors contain scoped immutable identities, never
//! locators or client-authored media metadata. Planning does not publish a revision.
use crate::{AssetVersion, FrameRate, Time, TimeRange, project_public::ApplyProject, scene::*};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Remove logical bindings and their media elements from every scene. Native
/// application planning owns dependent cleanup; stored media/history are kept.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MediaRemovalRequest {
    pub project_id: String,
    pub expected_revision: String,
    pub idempotency_key: String,
    pub label: String,
    pub asset_ids: Vec<String>,
}

/// Workspace placement may reuse this fixed revision's assets without another
/// media binding. Newly declared tracks and elements share one public apply.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MediaImportRequest {
    pub project_id: String,
    pub expected_revision: String,
    pub sequence_id: String,
    pub idempotency_key: String,
    pub label: String,
    pub bindings: Vec<MediaBinding>,
    pub placements: Vec<MediaPlacement>,
    pub ripple: bool,
    #[serde(default)]
    pub new_tracks: Vec<MediaTrack>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MediaTrack {
    pub id: String,
    pub name: String,
    /// Application planning accepts only Video, Audio and Graphic tracks.
    pub kind: SceneTrackKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MediaBinding {
    pub media_id: String,
    pub version: AssetVersion,
    pub asset_id: String,
    pub name: String,
    pub mode: MediaBindingMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MediaBindingMode {
    Bind,
    /// Preserve the logical asset ID and every dependent node. The replacement
    /// must have the same asset kind and satisfy the complete aggregate.
    Replace,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MediaPlacement {
    pub element_id: String,
    pub name: String,
    /// Existing explicit track, not an automatic placement or new-track hint.
    pub track_id: String,
    pub asset_id: String,
    /// Sequence presentation time, aligned nearest with positive ties upward.
    pub start: Time,
    pub content: MediaPlacementContent,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MediaPlacementContent {
    Video {
        timing: MediaTiming,
        source_audio_enabled: bool,
    },
    Audio {
        timing: MediaTiming,
    },
    Image {
        duration: Time,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MediaTiming {
    /// Preserve the requested source extent; derive timeline duration once
    /// using the profile's inverse mapping. Classic uses 120000 source ticks.
    SourceRange {
        range: TimeRange,
        rate: RationalRate,
        maintain_pitch: bool,
    },
    /// Preserve exact timeline duration; derive source duration once using the
    /// profile's forward mapping. This is not an inverse-rounded roundtrip.
    PinnedTimeline {
        source_start: Time,
        duration: Time,
        rate: RationalRate,
        maintain_pitch: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MediaImportPlan {
    /// Submit unchanged using public V4 apply; every selector is resolved again
    /// and the fixed expected head/authorization is rechecked before publication.
    pub transaction: ApplyProject,
    /// Digest of this original planning request. A read-only plan has no receipt;
    /// public apply's receipt binds the returned transaction, not this digest.
    pub original_request_digest: String,
    pub decisions: MediaImportDecisions,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MediaImportDecisions {
    pub settings_scope: MediaSettingsScope,
    pub previous_fps: FrameRate,
    /// Binding-import FPS decision before placement. Classic compares raw FPS
    /// first; normalization can make this slightly lower than previous_fps.
    pub raised_fps: Option<FrameRate>,
    /// Classic's empty-scene first video overwrites the global FPS after that
    /// element is placed, even when lower. Later placements use this clock.
    pub first_visual_fps: Option<FrameRate>,
    pub canvas_change: Option<MediaCanvasChange>,
    pub placements: Vec<MediaPlacementDecision>,
    pub bound_asset_ids: Vec<String>,
    pub replaced_asset_ids: Vec<String>,
    pub video_asset_ids_without_fps: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "profile", rename_all = "snake_case", deny_unknown_fields)]
pub enum MediaSettingsScope {
    ClassicGlobal,
    /// No implicit conversion to Classic global settings or geometry.
    LegacyPerScene {
        sequence_id: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MediaCanvasChange {
    pub previous: CanvasSize,
    pub next: CanvasSize,
    pub source_asset_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MediaPlacementDecision {
    pub element_id: String,
    pub requested_start: Time,
    pub resolved_start: Time,
    pub timeline_duration: Time,
    pub source_range: Option<TimeRange>,
    pub main_track_start_adjusted: bool,
}
