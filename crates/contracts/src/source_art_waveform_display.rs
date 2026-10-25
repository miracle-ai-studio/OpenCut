//! Classic clip waveform display facts, separate from frozen source peaks.
use crate::{AssetVersion, source_art::SourceArtIdentity};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceArtWaveformDisplayRequest {
    pub project_id: String,
    pub revision: String,
    pub asset_id: String,
    pub request_id: String,
    pub sample_rate: u32,
    pub sequence_id: String,
    pub track_id: String,
    pub element_id: String,
    pub viewport: SourceArtWaveformViewport,
    /// Optional temporary static element dB gain, in integer tenths. This
    /// never changes the fixed snapshot, mute, track or source-audio state.
    pub temporary_gain: Option<i32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceArtWaveformViewport {
    /// Observed CSS coordinates relative to the whole clip's left edge.
    pub clip_left_px: f64,
    pub clip_right_px: f64,
    pub pixels_per_second: f64,
    pub css_height: f64,
    pub device_pixel_ratio: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceArtWaveformDisplayResponse {
    pub project_id: String,
    pub revision: String,
    pub asset_id: String,
    pub sequence_id: String,
    pub track_id: String,
    pub element_id: String,
    pub version: AssetVersion,
    pub identity: SourceArtIdentity,
    /// classic-waveform-display-v1: 200 whole-clip midpoint gain samples,
    /// physical-amplitude interpolation and the Classic IEEE pixel clock.
    pub display_profile: String,
    pub sample_rate: u32,
    pub channels: u32,
    pub total_samples: String,
    pub bucket_size: u32,
    pub viewport: SourceArtWaveformViewport,
    pub temporary_gain: Option<i32>,
    pub canvas: SourceArtWaveformCanvas,
    pub bars: Vec<SourceArtWaveformBar>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceArtWaveformCanvas {
    /// Backing-store pixels. RGBA-equivalent allocation is bounded to 32 MiB.
    pub width: u32,
    pub height: u32,
    pub css_left: f64,
    pub css_width: f64,
    pub css_height: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceArtWaveformBar {
    /// Actual source sample window before expansion to 128-sample buckets.
    /// An empty or beyond-EOF window has peak zero. Decimal strings are u64.
    pub source_start_sample: String,
    pub source_end_sample: String,
    pub peak: f32,
    pub gain: f64,
    pub output_amplitude: f64,
    pub height_fraction: f64,
    pub burn: bool,
    pub device_left: u32,
    pub device_right: u32,
    /// Signed because Classic's minimum 1 CSS pixel bar can exceed a
    /// subpixel-height canvas. The canvas clips the resulting rectangle.
    pub device_top: i64,
    pub device_height: u32,
    pub burn_height: u32,
}
