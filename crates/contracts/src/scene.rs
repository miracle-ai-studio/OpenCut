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

use crate::scene_bookmarks::SceneBookmark;
use crate::{FrameRate, TimeRange};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditorDocument {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub default_sequence_id: String,
    pub settings: SettingsProfile,
    /// Absent means automatic artwork. References use this revision's pinned
    /// project assets; source time is independent of timeline trimming/retiming.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<ProjectCover>,
    pub assets: Vec<ProjectAsset>,
    pub fonts: Vec<FontResource>,
    pub sequences: Vec<Scene>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectCover {
    Image { asset_id: String },
    VideoFrame { asset_id: String, time: crate::Time },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditorSnapshot {
    pub project: EditorDocument,
    pub revision: String,
}

/// New and Classic-imported projects use ClassicGlobal. LegacyPerScene is an
/// explicit preservation profile for heterogeneous old Rust projects, never a
/// hidden override of Classic settings. Converting its scope is a separate edit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "profile", rename_all = "snake_case", deny_unknown_fields)]
pub enum SettingsProfile {
    ClassicGlobal { settings: ProjectSettings },
    LegacyPerScene { formats: Vec<LegacySequenceFormat> },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectSettings {
    pub fps: FrameRate,
    pub canvas_size: CanvasSize,
    pub canvas_size_mode: CanvasSizeMode,
    pub last_custom_canvas_size: Option<CanvasSize>,
    pub original_canvas_size: Option<CanvasSize>,
    pub background: Background,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LegacySequenceFormat {
    pub sequence_id: String,
    pub fps: FrameRate,
    pub canvas_size: CanvasSize,
    pub background: Background,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CanvasSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CanvasSizeMode {
    Preset,
    Custom,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Scene {
    pub id: String,
    pub name: String,
    pub main_track_id: String,
    /// Visual/effect track order is bottom to top, independent of insertion ID.
    pub tracks: Vec<SceneTrack>,
    pub captions: Vec<Caption>,
    pub transitions: Vec<SceneTransition>,
    pub bookmarks: Vec<SceneBookmark>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SceneTrackKind {
    Video,
    Audio,
    Text,
    Graphic,
    Effect,
    Caption,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SceneTrack {
    pub id: String,
    pub name: String,
    pub kind: SceneTrackKind,
    pub enabled: bool,
    pub muted: bool,
    pub locked: bool,
    pub opacity: f64,
    pub gain: Gain,
    /// Stable element order is also the layer order for overlapping legacy
    /// Graphic tracks. Classic tracks retain non-overlapping edit semantics.
    pub elements: Vec<TimelineElement>,
    /// Track-owned channels use sequence time, never an arbitrary element clock.
    pub channels: Vec<AnimationChannel>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimelineElement {
    pub id: String,
    pub name: String,
    pub timeline_range: TimeRange,
    /// The only source/local span authority. Media content refers to this span;
    /// it does not carry a second independently writable source range.
    pub source_span: ElementSourceSpan,
    pub enabled: bool,
    pub content: ElementContent,
    /// Audio-only elements have no visual style.
    pub visual: Option<VisualStyle>,
    pub channels: Vec<AnimationChannel>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ElementContent {
    Video {
        asset_id: String,
        video_stream: Option<u32>,
        source_audio_enabled: bool,
        audio: AudioStyle,
        retime: Retime,
    },
    Audio {
        asset_id: String,
        audio: AudioStyle,
        retime: Retime,
    },
    Image {
        asset_id: String,
    },
    Text {
        text: Text,
    },
    Sticker {
        asset_id: String,
        intrinsic_size: CanvasSize,
    },
    Graphic {
        graphic: Graphic,
    },
    /// Applies at this track's layer boundary to the already composited layers
    /// below, including background. It is not copied onto each lower element.
    EffectLayer {
        effect: SceneEffect,
    },
}

#[cfg(test)]
mod tests;
