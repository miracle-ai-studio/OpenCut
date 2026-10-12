//! Unpublished, revision-owned editor aggregate. Declaring a node does not imply
//! compiler or renderer support. Existing project and wire schemas stay frozen.
//!
//! Execution references are logical CAS identities. Legacy locator provenance
//! may retain a host path in this stored model; a separate public view is needed
//! before publication. Decoder handles, selection and preview state stay outside.
mod animation;
mod media;
mod text;
mod visual;

pub use animation::*;
pub use media::*;
pub use text::*;
pub use visual::*;

use crate::scene_bookmarks::SceneBookmarkV1;
use crate::{FrameRate, TimeRange};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectDocumentV4 {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub default_sequence_id: String,
    pub settings: SettingsProfileV4,
    pub assets: Vec<AssetV4>,
    pub fonts: Vec<FontResourceV4>,
    pub sequences: Vec<SequenceV4>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectSnapshotV4 {
    pub project: ProjectDocumentV4,
    pub revision: String,
}

/// New and Classic-imported projects use ClassicGlobal. LegacyPerScene is an
/// explicit preservation profile for heterogeneous old Rust projects, never a
/// hidden override of Classic settings. Converting its scope is a separate edit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "profile", rename_all = "snake_case", deny_unknown_fields)]
pub enum SettingsProfileV4 {
    ClassicGlobal {
        settings: ProjectSettingsV4,
    },
    LegacyPerScene {
        formats: Vec<LegacySequenceFormatV4>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectSettingsV4 {
    pub fps: FrameRate,
    pub canvas_size: CanvasSizeV4,
    pub canvas_size_mode: CanvasSizeModeV4,
    pub last_custom_canvas_size: Option<CanvasSizeV4>,
    pub original_canvas_size: Option<CanvasSizeV4>,
    pub background: BackgroundV4,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LegacySequenceFormatV4 {
    pub sequence_id: String,
    pub fps: FrameRate,
    pub canvas_size: CanvasSizeV4,
    pub background: BackgroundV4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CanvasSizeV4 {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CanvasSizeModeV4 {
    Preset,
    Custom,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SequenceV4 {
    pub id: String,
    pub name: String,
    pub main_track_id: String,
    /// Visual/effect track order is bottom to top, independent of insertion ID.
    pub tracks: Vec<TrackV4>,
    pub captions: Vec<CaptionV4>,
    pub transitions: Vec<TransitionV4>,
    pub bookmarks: Vec<SceneBookmarkV1>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TrackKindV4 {
    Video,
    Audio,
    Text,
    Graphic,
    Effect,
    Caption,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TrackV4 {
    pub id: String,
    pub name: String,
    pub kind: TrackKindV4,
    pub enabled: bool,
    pub muted: bool,
    pub locked: bool,
    pub opacity: f64,
    pub gain: GainV4,
    /// Stable element order is also the layer order for overlapping legacy
    /// Graphic tracks. Classic tracks retain non-overlapping edit semantics.
    pub elements: Vec<TimelineElementV4>,
    /// Track-owned channels use sequence time, never an arbitrary element clock.
    pub channels: Vec<AnimationChannelV4>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimelineElementV4 {
    pub id: String,
    pub name: String,
    pub timeline_range: TimeRange,
    /// The only source/local span authority. Media content refers to this span;
    /// it does not carry a second independently writable source range.
    pub source_span: ElementSourceSpanV4,
    pub enabled: bool,
    pub content: ElementContentV4,
    /// Audio-only elements have no visual style.
    pub visual: Option<VisualStyleV4>,
    pub channels: Vec<AnimationChannelV4>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ElementContentV4 {
    Video {
        asset_id: String,
        video_stream: Option<u32>,
        source_audio_enabled: bool,
        audio: AudioStyleV4,
        retime: RetimeV4,
    },
    Audio {
        asset_id: String,
        audio: AudioStyleV4,
        retime: RetimeV4,
    },
    Image {
        asset_id: String,
    },
    Text {
        text: TextV4,
    },
    Sticker {
        asset_id: String,
        intrinsic_size: CanvasSizeV4,
    },
    Graphic {
        graphic: GraphicV4,
    },
    /// Applies at this track's layer boundary to the already composited layers
    /// below, including background. It is not copied onto each lower element.
    EffectLayer {
        effect: EffectV4,
    },
}

#[cfg(test)]
mod tests;
