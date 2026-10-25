//! Logical media/font resources, gain units and source-clock profiles.
use super::{CanvasSize, FontStyle};
use crate::{AssetKind, AssetVersion, FontResolutionPolicy, FrameRate, Time, TimeRange};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectAsset {
    pub id: String,
    pub name: String,
    pub kind: AssetKind,
    pub version: AssetVersion,
    pub duration: Option<Time>,
    pub origin: ResourceOrigin,
    pub metadata: ProjectMediaMetadata,
    pub license: Option<ResourceLicense>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ResourceOrigin {
    LocalImport {
        original_name: String,
    },
    Online {
        provider: String,
        source_url: String,
    },
    /// Provenance only. The CAS version remains the rendering authority.
    LegacyLocator {
        uri: String,
    },
    Generated {
        provider: String,
        generation_id: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResourceLicense {
    pub identifier: Option<String>,
    pub source_url: Option<String>,
    pub notice: Option<String>,
    pub attribution: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectMediaMetadata {
    pub mime_type: Option<String>,
    pub intrinsic_size: Option<CanvasSize>,
    pub frame_rate: Option<FrameRate>,
    pub rotation_degrees: Option<f64>,
    pub audio: AudioPresence,
}

/// Probe knowledge is independent of the video's source-audio preference.
/// Unknown may be preserved in an imported revision, but is not evidence of an
/// audio stream. Absent is a confirmed probe result, not a missing metadata row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum AudioPresence {
    Unknown {},
    Absent {},
    /// Older probes recorded presence without the stream format. None preserves
    /// that knowledge; execution still probes the required concrete format.
    Present {
        metadata: Option<AudioMetadata>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioMetadata {
    pub sample_rate: u32,
    pub channels: u32,
    pub channel_layout: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontResource {
    pub id: String,
    pub family: String,
    /// OpenType/CSS numeric weight, including variable-font instances.
    pub weight: u16,
    pub style: FontStyle,
    pub face_index: u32,
    pub content: AssetVersion,
    pub origin: ResourceOrigin,
    pub license: Option<ResourceLicense>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FontBinding {
    Locked {
        font_id: String,
    },
    /// Preserves unresolved family semantics on import. A renderer must report
    /// resolution/identity; saving this is not proof of reproducible output.
    UnresolvedFamily {
        family: String,
        policy: FontResolutionPolicy,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioStyle {
    pub stream: Option<u32>,
    pub gain: Gain,
    pub muted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "unit", rename_all = "snake_case", deny_unknown_fields)]
pub enum Gain {
    /// Preserves zero and arbitrarily quiet legacy nonzero gains. Channels
    /// interpolate linear amplitude, as the old project contract specified.
    LinearLegacy { value: f64 },
    /// Classic default 0 dB, UI -60..20; amplitude is 10^(value/20).
    Decibels { value: f64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RationalRate {
    pub numerator: u32,
    pub denominator: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Retime {
    pub rate: RationalRate,
    pub maintain_pitch: bool,
    pub mapping: RetimeMapping,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RetimeMapping {
    ExactRational,
    /// Visible source duration = round(timeline_duration*rate*timebase)/timebase,
    /// positive ties upward. A rate-edit derives a new timeline duration from
    /// the preserved source extent using the inverse operation, once. These
    /// operations are quantized and must not be assumed to be exact inverses.
    ClassicRoundedTicks {
        timebase: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "profile", rename_all = "snake_case", deny_unknown_fields)]
pub enum ElementSourceSpan {
    /// Classic keeps trims on generated/static elements too: their pixels do
    /// not sample this span, but split/resize/recovery still need it. For media,
    /// source start=trim_start; visible source duration is timeline duration
    /// multiplied by rate and rounded once to the Classic tick grid. Animated
    /// properties always use timeline-local time, without adding trim_start.
    /// source_duration is the original extent/resize ceiling, not a second
    /// visible range; None preserves Classic's inferred, extendable extent.
    ClassicTrimmed {
        trim_start: Time,
        trim_end: Time,
        source_duration: Option<Time>,
    },
    /// Exact old Rust media span; the referenced asset supplies the full extent.
    LegacySourceRange { range: TimeRange },
}
