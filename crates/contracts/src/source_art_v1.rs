//! Revision-scoped source art. Peaks describe decoded source samples, without
//! element gain, mastering or the Classic waveform's burn/display curve.
use crate::{AssetVersion, Time, TimeRange};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceArtRequestV1 {
    pub project_id: String,
    pub revision: String,
    pub asset_id: String,
    /// A scoped waiter identity; it is neither an edit nor a persistent job.
    pub request_id: String,
    pub query: SourceArtQueryV1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SourceArtQueryV1 {
    Waveform {
        /// Explicit AudioContext rate, 44100 or 48000. Classic's device-default
        /// choice is made explicit so summaries cannot cross decoding profiles.
        sample_rate: u32,
        projection: WaveformProjectionV1,
    },
    /// Primary video sample at one second, or a static image; Classic's
    /// sequential fit to 1280 x 720, no upscale, canvas JPEG quality 0.8.
    Thumbnail {},
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WaveformProjectionV1 {
    Source {
        /// None covers the actual decoded extent, rather than nominal metadata.
        range: Option<TimeRange>,
        bins: u32,
    },
    Element {
        sequence_id: String,
        track_id: String,
        element_id: String,
        /// Placement-local window. Rust resolves trim and effective source rate.
        local_range: TimeRange,
        bins: u32,
        /// Positive placement-local step; bins = ceil(duration / step).
        /// The last bin ends at the requested local window end.
        bin_duration: Time,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceArtCancelV1 {
    pub project_id: String,
    pub revision: String,
    pub request_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceArtCancelResponseV1 {
    pub cancel_requested: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceArtIdentityV1 {
    /// source-peaks128-v1 or classic-thumbnail-1s-jpeg08-v1.
    pub algorithm: String,
    pub browser_version: String,
    pub browser_sha256: String,
    pub node_sha256: String,
    /// Hash of the complete pinned source-art renderer module manifest.
    pub module_sha256: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceArtResponseV1 {
    pub schema_version: u32,
    pub project_id: String,
    pub revision: String,
    pub asset_id: String,
    pub version: AssetVersion,
    pub identity: SourceArtIdentityV1,
    pub data: SourceArtDataV1,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SourceArtDataV1 {
    Waveform {
        sample_rate: u32,
        channels: u32,
        /// Canonical unsigned decimal, independent of JavaScript's safe integer.
        total_samples: String,
        bucket_size: u32,
        projection: WaveformProjectionV1,
        /// Max of intersecting 128-sample source buckets across all channels.
        amplitudes: Vec<f32>,
    },
    ThumbnailReady {
        width: u32,
        height: u32,
        content_hash: String,
        /// Lowercase hexadecimal of at most 1 MiB of verified JPEG bytes.
        jpeg_bytes_hex: String,
        observation: SourceArtThumbnailObservationV1,
    },
    ThumbnailNoFrame {
        observation: SourceArtThumbnailObservationV1,
        reason: SourceArtNoFrameReasonV1,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SourceArtThumbnailObservationV1 {
    StaticImage {
        source_width: u32,
        source_height: u32,
    },
    Video {
        track_id: u32,
        source_width: u32,
        source_height: u32,
        requested_seconds: f64,
        /// Observed browser seconds, not exact project ticks. Present only
        /// when getSample(1) returned a real sample; no synthetic hold frame.
        actual_timestamp_seconds: Option<f64>,
        frame_duration_seconds: Option<f64>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SourceArtNoFrameReasonV1 {
    NoSampleAtOneSecond,
}
