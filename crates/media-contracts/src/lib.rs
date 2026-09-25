//! Portable media wire types. Default location values are UTF-8 strings.
//! Native backends instantiate the location parameter with their own path type.

pub mod analysis;
pub mod composition;
pub mod composition_v3;
pub use analysis::*;
pub mod speaker_turns;
pub mod transcription;
pub use speaker_turns::*;
pub use transcription::{
    TranscribeRequest, TranscriptionResult, TranscriptionSegment, TranscriptionWord,
    WordTimingStatus, validate_public_transcription_result,
};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MediaTime {
    #[serde(with = "string_i64")]
    #[schemars(with = "String")]
    pub ticks: i64,
    pub timebase: u32,
}

impl MediaTime {
    pub fn new(ticks: i64, timebase: u32) -> Result<Self, String> {
        if timebase == 0 {
            Err("timebase must be greater than zero".into())
        } else {
            Ok(Self { ticks, timebase })
        }
    }
    pub fn seconds(self) -> f64 {
        self.ticks as f64 / self.timebase as f64
    }
}

mod string_i64 {
    use serde::{Deserialize, Deserializer, Serializer, de};
    pub fn serialize<S: Serializer>(value: &i64, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&value.to_string())
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<i64, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FrameRate {
    pub numerator: u32,
    pub denominator: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "ProbeRequest")]
pub struct ProbeRequest<P = String> {
    pub input: P,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[schemars(rename = "MediaMetadata")]
pub struct MediaMetadata<P = String> {
    pub input: P,
    pub format_name: Option<String>,
    /// ISO Base Media File Format major brand when the container exposes one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format_brand: Option<String>,
    pub duration: Option<MediaTime>,
    pub bit_rate: Option<u64>,
    pub streams: Vec<MediaStream>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct MediaStream {
    pub index: u32,
    pub codec_type: StreamKind,
    pub codec_name: Option<String>,
    pub duration: Option<MediaTime>,
    pub time_base: Option<FrameRate>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u32>,
    pub average_frame_rate: Option<FrameRate>,
    pub pixel_format: Option<String>,
    pub channel_layout: Option<String>,
    pub color_space: Option<String>,
    pub color_transfer: Option<String>,
    pub color_primaries: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StreamKind {
    Video,
    Audio,
    Subtitle,
    Data,
    Attachment,
    Unknown,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FrameAlignment {
    Exact,
    Nearest,
    Floor,
    Ceil,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[schemars(rename = "ExtractedFrame")]
pub struct ExtractedFrame<P = String> {
    pub output: P,
    pub requested_time: MediaTime,
    pub actual_time: MediaTime,
    pub source_pts: i64,
    pub source_time_base: FrameRate,
    pub alignment: FrameAlignment,
    pub exact: bool,
    pub quality: FrameQuality,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FrameQuality {
    AccurateSourceFrame,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "ExportSegment")]
pub struct ExportSegment<P = String> {
    pub input: P,
    pub source_start: MediaTime,
    pub source_duration: MediaTime,
    #[serde(default)]
    pub video_stream: Option<u32>,
    #[serde(default)]
    pub audio_stream: Option<u32>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Container {
    Mp4,
    Matroska,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct VideoOutput {
    pub width: u32,
    pub height: u32,
    pub frame_rate: FrameRate,
    pub aspect_mode: AspectMode,
    pub codec: VideoCodec,
    #[serde(default)]
    pub crf: Option<u8>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AspectMode {
    Fit,
    Fill,
    Stretch,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VideoCodec {
    H264,
    Hevc,
    Vp9,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum AudioOutput {
    Preserve {
        #[serde(default)]
        gain_db: f32,
    },
    Drop,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[schemars(rename = "ExportResult")]
pub struct ExportResult<P = String> {
    pub output: P,
    pub metadata: MediaMetadata<P>,
    pub verification: VerificationReport<P>,
    #[serde(default)]
    pub text_layouts: Vec<TextLayoutReport>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum FontResolutionPolicy {
    Strict {},
    ExplicitFallback { families: Vec<String> },
}

impl Default for FontResolutionPolicy {
    fn default() -> Self {
        Self::Strict {}
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
    Left,
    #[default]
    Center,
    Right,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct TextLayoutReport {
    pub requested_family: String,
    pub policy: FontResolutionPolicy,
    pub used_faces: Vec<UsedFontFace>,
    pub line_count: u32,
    pub glyph_count: u32,
    pub rtl_runs: u32,
    pub cluster_ranges: Vec<TextClusterRange>,
    pub pixel_bounds: PixelBounds,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct UsedFontFace {
    pub family: String,
    pub postscript_name: String,
    /// Compact origin identifier: `file:<path>` or `binary:opaque:<length>bytes`.
    /// Embedded font bytes are never included.
    pub source: String,
    pub face_index: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct TextClusterRange {
    pub start: usize,
    pub end: usize,
    pub rtl: bool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct PixelBounds {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "VerifyRequest")]
pub struct VerifyRequest<P = String> {
    pub input: P,
    #[serde(default)]
    pub expected_width: Option<u32>,
    #[serde(default)]
    pub expected_height: Option<u32>,
    #[serde(default)]
    pub expected_duration: Option<MediaTime>,
    #[serde(default)]
    pub duration_tolerance: Option<MediaTime>,
    #[serde(default)]
    pub expected_video_streams: Option<u32>,
    #[serde(default)]
    pub expected_audio_streams: Option<u32>,
    #[serde(default = "default_samples")]
    pub decode_samples: u32,
}
fn default_samples() -> u32 {
    3
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[schemars(rename = "VerificationReport")]
pub struct VerificationReport<P = String> {
    pub passed: bool,
    pub checks: Vec<VerificationCheck>,
    pub metadata: MediaMetadata<P>,
    pub decoded_samples: Vec<DecodedSample>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct VerificationCheck {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct DecodedSample {
    pub requested_time: MediaTime,
    pub decoded: bool,
}

impl<P> ProbeRequest<P> {
    pub fn try_map_paths<Q, E>(
        self,
        mut map: impl FnMut(P) -> Result<Q, E>,
    ) -> Result<ProbeRequest<Q>, E> {
        Ok(ProbeRequest {
            input: map(self.input)?,
        })
    }
    pub fn map_paths<Q>(self, mut map: impl FnMut(P) -> Q) -> ProbeRequest<Q> {
        self.try_map_paths(|path| Ok::<Q, std::convert::Infallible>(map(path)))
            .expect("infallible location mapping")
    }
}

impl<P> MediaMetadata<P> {
    pub fn try_map_paths<Q, E>(
        self,
        mut map: impl FnMut(P) -> Result<Q, E>,
    ) -> Result<MediaMetadata<Q>, E> {
        Ok(MediaMetadata {
            input: map(self.input)?,
            format_name: self.format_name,
            format_brand: self.format_brand,
            duration: self.duration,
            bit_rate: self.bit_rate,
            streams: self.streams,
        })
    }
    pub fn map_paths<Q>(self, mut map: impl FnMut(P) -> Q) -> MediaMetadata<Q> {
        self.try_map_paths(|path| Ok::<Q, std::convert::Infallible>(map(path)))
            .expect("infallible location mapping")
    }
}

impl<P> ExtractedFrame<P> {
    pub fn try_map_paths<Q, E>(
        self,
        mut map: impl FnMut(P) -> Result<Q, E>,
    ) -> Result<ExtractedFrame<Q>, E> {
        Ok(ExtractedFrame {
            output: map(self.output)?,
            requested_time: self.requested_time,
            actual_time: self.actual_time,
            source_pts: self.source_pts,
            source_time_base: self.source_time_base,
            alignment: self.alignment,
            exact: self.exact,
            quality: self.quality,
        })
    }
    pub fn map_paths<Q>(self, mut map: impl FnMut(P) -> Q) -> ExtractedFrame<Q> {
        self.try_map_paths(|path| Ok::<Q, std::convert::Infallible>(map(path)))
            .expect("infallible location mapping")
    }
}

impl<P> ExportSegment<P> {
    pub fn try_map_paths<Q, E>(
        self,
        mut map: impl FnMut(P) -> Result<Q, E>,
    ) -> Result<ExportSegment<Q>, E> {
        Ok(ExportSegment {
            input: map(self.input)?,
            source_start: self.source_start,
            source_duration: self.source_duration,
            video_stream: self.video_stream,
            audio_stream: self.audio_stream,
        })
    }
    pub fn map_paths<Q>(self, mut map: impl FnMut(P) -> Q) -> ExportSegment<Q> {
        self.try_map_paths(|path| Ok::<Q, std::convert::Infallible>(map(path)))
            .expect("infallible location mapping")
    }
}

impl<P> ExportResult<P> {
    pub fn try_map_paths<Q, E>(
        self,
        mut map: impl FnMut(P) -> Result<Q, E>,
    ) -> Result<ExportResult<Q>, E> {
        Ok(ExportResult {
            output: map(self.output)?,
            metadata: self.metadata.try_map_paths(&mut map)?,
            verification: self.verification.try_map_paths(&mut map)?,
            text_layouts: self.text_layouts,
        })
    }
    pub fn map_paths<Q>(self, mut map: impl FnMut(P) -> Q) -> ExportResult<Q> {
        self.try_map_paths(|path| Ok::<Q, std::convert::Infallible>(map(path)))
            .expect("infallible location mapping")
    }
}

impl<P> VerifyRequest<P> {
    pub fn try_map_paths<Q, E>(
        self,
        mut map: impl FnMut(P) -> Result<Q, E>,
    ) -> Result<VerifyRequest<Q>, E> {
        Ok(VerifyRequest {
            input: map(self.input)?,
            expected_width: self.expected_width,
            expected_height: self.expected_height,
            expected_duration: self.expected_duration,
            duration_tolerance: self.duration_tolerance,
            expected_video_streams: self.expected_video_streams,
            expected_audio_streams: self.expected_audio_streams,
            decode_samples: self.decode_samples,
        })
    }
    pub fn map_paths<Q>(self, mut map: impl FnMut(P) -> Q) -> VerifyRequest<Q> {
        self.try_map_paths(|path| Ok::<Q, std::convert::Infallible>(map(path)))
            .expect("infallible location mapping")
    }
}

impl<P> VerificationReport<P> {
    pub fn try_map_paths<Q, E>(
        self,
        mut map: impl FnMut(P) -> Result<Q, E>,
    ) -> Result<VerificationReport<Q>, E> {
        Ok(VerificationReport {
            passed: self.passed,
            checks: self.checks,
            metadata: self.metadata.try_map_paths(&mut map)?,
            decoded_samples: self.decoded_samples,
        })
    }
    pub fn map_paths<Q>(self, mut map: impl FnMut(P) -> Q) -> VerificationReport<Q> {
        self.try_map_paths(|path| Ok::<Q, std::convert::Infallible>(map(path)))
            .expect("infallible location mapping")
    }
}
