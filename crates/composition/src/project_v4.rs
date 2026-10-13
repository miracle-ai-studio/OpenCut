//! Independent fixed-revision, per-frame V4 execution graph. No V3 projection,
//! files, decoders or renderer capability claims live in this compiler.
use crate::{CompositionError, CompositionScope};
use opencut_contracts::{
    AssetKind, AssetVersion, EditorError, FrameRate, Time, TimeRange, project_v4::*,
};
use opencut_editor_core::project_v4::{
    SampledChannelV4, apply_sample, element_local_time_v4, gain_amplitude,
    sample_animation_channel_v4, validate_project_v4,
};
use opencut_time::RationalTime;
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
};

mod animation;
mod audio;
pub use audio::*;
mod capabilities;
mod compiler;
mod coverage;
mod source;
#[cfg(test)]
mod tests;
mod text_execution;
mod text_frame_execution;
mod visual_execution;
pub use capabilities::*;
pub use compiler::compile_frame_v4;
pub use coverage::*;
pub use source::*;
pub use text_execution::*;
pub use text_frame_execution::*;
pub use visual_execution::*;

#[derive(Clone, Debug, PartialEq)]
pub struct CompositionV4 {
    pub revision: String,
    pub project_id: String,
    pub sequence_id: String,
    pub scope: CompositionScope,
    /// Exact caller-selected presentation time, not converted through seconds.
    pub time: Time,
    pub format: CompiledFormatV4,
    pub tracks: Vec<CompiledTrackV4>,
    /// Bottom-to-top execution order; effect layers act on the accumulated image.
    pub layers: Vec<CompiledLayerV4>,
    pub audio: Vec<CompiledAudioV4>,
    pub transitions: Vec<CompiledTransitionV4>,
    pub background: CompiledBackgroundV4,
    pub resources: Vec<CasResourceV4>,
    pub fonts: Vec<FontResourceV4>,
    pub coverage: Vec<SourceCoverageV4>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompiledFormatV4 {
    pub fps: FrameRate,
    pub canvas_size: CanvasSizeV4,
    pub profile: FormatProfileV4,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormatProfileV4 {
    ClassicGlobal,
    LegacyPerScene,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CasResourceV4 {
    pub asset_id: String,
    pub version: AssetVersion,
    pub kind: AssetKind,
    pub duration: Option<Time>,
    pub metadata: MediaMetadataV4,
    pub license: Option<ResourceLicenseV4>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompiledTrackV4 {
    pub id: String,
    pub order: usize,
    pub kind: TrackKindV4,
    pub opacity: f64,
    /// Visual-only scope intentionally has no evaluated audio gain.
    pub gain: Option<CompiledGainV4>,
    pub muted: bool,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompiledGainV4 {
    /// Physical projection in the authored unit, followed by amplitude conversion.
    pub physical: GainV4,
    pub amplitude: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompiledLayerV4 {
    pub element_id: String,
    pub track_id: String,
    pub track_order: usize,
    pub element_order: usize,
    pub local_time: Time,
    pub visual: Option<SampledVisualV4>,
    pub content: SampledContentV4,
}
#[derive(Clone, Debug, PartialEq)]
pub struct SampledVisualV4 {
    pub profile: VisualProfileV4,
    pub transform: TransformV4,
    pub element_opacity: f64,
    pub track_opacity: f64,
    /// Legacy shapes deliberately bypass track opacity, as old V3 rendering did.
    pub effective_opacity: f64,
    pub opacity_policy: LayerOpacityPolicyV4,
    pub blend_mode: BlendModeV4,
    pub effects: Vec<EffectV4>,
    pub masks: Vec<MaskV4>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerOpacityPolicyV4 {
    MultiplyTrack,
    LegacyShapeElementOnly,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SampledContentV4 {
    Video {
        asset_id: String,
        stream: Option<u32>,
        source: SourceClockV4,
        source_audio: SourceAudioDecisionV4,
    },
    Image {
        asset_id: String,
    },
    Sticker {
        asset_id: String,
        intrinsic_size: CanvasSizeV4,
    },
    Text {
        text: TextV4,
    },
    Graphic {
        graphic: GraphicV4,
    },
    Caption {
        text: String,
        style: CaptionStyleV4,
    },
    EffectLayer {
        effect: EffectV4,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub enum SourceAudioDecisionV4 {
    ScopeExcluded,
    Disabled,
    Muted,
    KnownAbsent,
    RequiresProbe,
    Emitted,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompiledAudioV4 {
    pub element_id: String,
    pub track_id: String,
    pub asset_id: String,
    pub stream: Option<u32>,
    pub source: SourceClockV4,
    pub presence: AudioPresenceV4,
    pub maintain_pitch: bool,
    pub element_gain: CompiledGainV4,
    pub track_gain: CompiledGainV4,
    pub amplitude: f64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct CompiledTransitionV4 {
    pub id: String,
    pub outgoing_element_id: String,
    pub incoming_element_id: String,
    pub profile: TransitionKindV4,
    pub range: TimeRange,
    pub incoming_weight: f64,
    /// Legacy V3 leaves the outgoing layer intact; alpha-over of the incoming
    /// faded layer supplies the outgoing attenuation. Do not fade it twice.
    pub outgoing_weight: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CompiledBackgroundV4 {
    Color {
        color: ColorV4,
    },
    MainTrackBlur {
        intensity: f64,
        main_track_id: String,
        /// Raw media source nodes, not the finished foreground composition.
        source_layers: Vec<String>,
    },
}

fn error(value: EditorError) -> CompositionError {
    CompositionError {
        code: "INVALID_PROJECT",
        message: value.message,
    }
}
fn invalid(message: impl Into<String>) -> CompositionError {
    CompositionError {
        code: "INVALID_PROJECT",
        message: message.into(),
    }
}
fn unsupported(message: impl Into<String>) -> CompositionError {
    CompositionError {
        code: "UNSUPPORTED_V4",
        message: message.into(),
    }
}
fn time(value: &Time) -> Result<RationalTime, CompositionError> {
    RationalTime::try_from(value).map_err(error)
}
fn property(parameter: ScalarParameterV4, raw: f64) -> Result<f64, CompositionError> {
    apply_sample(&ScalarBindingV4::Property { parameter }, raw).map_err(error)
}
fn finite(value: f64) -> Result<f64, CompositionError> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(invalid("Compiled numeric value is not finite"))
    }
}
fn end(range: &TimeRange) -> Result<RationalTime, CompositionError> {
    time(&opencut_time::range_end(range).map_err(error)?)
}
fn contains(range: &TimeRange, at: RationalTime) -> Result<bool, CompositionError> {
    Ok(time(&range.start)?.cmp_exact(at) != Ordering::Greater
        && end(range)?.cmp_exact(at) == Ordering::Greater)
}

mod text_frame_execution_v3;
pub use text_frame_execution_v3::*;
