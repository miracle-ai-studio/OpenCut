use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FrameExtractRequest {
    pub input: PathBuf,
    pub output: PathBuf,
    pub requested_time: MediaTime,
    #[serde(default)]
    pub video_stream: Option<u32>,
    pub alignment: FrameAlignment,
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ExportRequest {
    pub segments: Vec<ExportSegment>,
    pub output: PathBuf,
    pub container: Container,
    pub video: VideoOutput,
    pub audio: AudioOutput,
    #[serde(default)]
    pub requested_features: Vec<String>,
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TimelineExportRequest {
    pub output: PathBuf,
    pub container: Container,
    pub video: VideoOutput,
    pub duration: MediaTime,
    #[serde(default)]
    pub video_clips: Vec<TimelineVideoClip>,
    #[serde(default)]
    pub image_clips: Vec<TimelineImageClip>,
    #[serde(default)]
    pub audio_clips: Vec<TimelineAudioClip>,
    #[serde(default)]
    pub captions: Vec<TimelineCaption>,
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TimelineFrameRenderRequest {
    pub timeline: TimelineExportRequest,
    pub sequence_time: MediaTime,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TimelineRangeRenderRequest {
    pub timeline: TimelineExportRequest,
    pub sequence_start: MediaTime,
    pub sequence_duration: MediaTime,
}

/// Schema 3 visual graph; `base` retains the exact schema 2 media and audio timeline.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TimelineExportRequestV3 {
    pub base: TimelineExportRequest,
    pub visual: opencut_media_contracts::composition_v3::CompiledVisualGraphV3,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TimelineFrameRenderRequestV3 {
    pub timeline: TimelineExportRequestV3,
    pub sequence_time: MediaTime,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TimelineRangeRenderRequestV3 {
    pub timeline: TimelineExportRequestV3,
    pub sequence_start: MediaTime,
    pub sequence_duration: MediaTime,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct RenderedTimelineFrame {
    pub output: PathBuf,
    pub actual_sequence_time: MediaTime,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub text_layouts: Vec<TextLayoutReport>,
}

pub use opencut_media_contracts::composition::{
    EvaluatedCurve, EvaluatedSegment, TimelineCaption, TimelineVisualCurves,
};
pub use opencut_media_contracts::composition_v3::*;
pub type TimelineVideoClip = opencut_media_contracts::composition::TimelineVideoClip<PathBuf>;
pub type TimelineImageClip = opencut_media_contracts::composition::TimelineImageClip<PathBuf>;
pub type TimelineAudioClip = opencut_media_contracts::composition::TimelineAudioClip<PathBuf>;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProgressStage {
    ResolvingAssets,
    Probing,
    SelectingFrame,
    Decoding,
    Encoding,
    Verifying,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct ProgressUpdate {
    pub stage: ProgressStage,
    pub fraction: Option<f32>,
    pub media_time: Option<MediaTime>,
}

// Native request/result aliases share the public fields and use native locations only here.
pub use opencut_media_contracts::{
    AspectMode, AudioOutput, Container, DecodedSample, FontResolutionPolicy, FrameAlignment,
    FrameQuality, FrameRate, MediaStream, MediaTime, PixelBounds, StreamKind, TextAlign,
    TextClusterRange, TextLayoutReport, UsedFontFace, VerificationCheck, VideoCodec, VideoOutput,
};
pub type ProbeRequest = opencut_media_contracts::ProbeRequest<PathBuf>;
pub type MediaMetadata = opencut_media_contracts::MediaMetadata<PathBuf>;
pub type ExtractedFrame = opencut_media_contracts::ExtractedFrame<PathBuf>;
pub type ExportSegment = opencut_media_contracts::ExportSegment<PathBuf>;
pub type ExportResult = opencut_media_contracts::ExportResult<PathBuf>;
pub type VerifyRequest = opencut_media_contracts::VerifyRequest<PathBuf>;
pub type VerificationReport = opencut_media_contracts::VerificationReport<PathBuf>;
pub type TranscribeRequest = opencut_media_contracts::TranscribeRequest<PathBuf>;
pub use opencut_media_contracts::validate_public_transcription_result;
pub use opencut_media_contracts::{
    ShotBoundary, ShotsResult, SilenceInterval, SilenceResult, WaveformBucket, WaveformResult,
};
pub type SilenceRequest = opencut_media_contracts::SilenceRequest<PathBuf>;
pub type ShotsRequest = opencut_media_contracts::ShotsRequest<PathBuf>;
pub type WaveformRequest = opencut_media_contracts::WaveformRequest<PathBuf>;
pub use opencut_media_contracts::{SpeakerTurn, SpeakerTurnsResult};
pub use opencut_media_contracts::{
    TranscriptionResult, TranscriptionSegment, TranscriptionWord, WordTimingStatus,
};
pub type SpeakerTurnsRequest = opencut_media_contracts::SpeakerTurnsRequest<PathBuf>;
