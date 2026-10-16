//! Additive Classic export choices. Existing fixed MP4 contracts remain frozen.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormatV2 {
    Mp4,
    Webm,
}
impl ExportFormatV2 {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::Webm => "webm",
        }
    }
    pub fn mime_type(self) -> &'static str {
        match self {
            Self::Mp4 => "video/mp4",
            Self::Webm => "video/webm",
        }
    }
    pub fn video_codec(self) -> &'static str {
        match self {
            Self::Mp4 => "h264",
            Self::Webm => "vp9",
        }
    }
    pub fn audio_codec(self) -> &'static str {
        match self {
            Self::Mp4 => "aac",
            Self::Webm => "opus",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExportQualityV2 {
    Low,
    Medium,
    High,
    VeryHigh,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportVideoOptionsV2 {
    pub format: ExportFormatV2,
    pub quality: ExportQualityV2,
    pub include_audio: bool,
}
impl Default for ExportVideoOptionsV2 {
    fn default() -> Self {
        Self {
            format: ExportFormatV2::Mp4,
            quality: ExportQualityV2::High,
            include_audio: true,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExportVisualProfileV2 {
    TextV2,
    CaptionV3,
    GraphicV1,
    BlendV1,
    MaskV1,
    SceneV1,
}

/// Numeric target bitrates from Classic's pinned Mediabunny 1.41.0 Quality.
/// These are encoder targets, not promises about final bytes or exact equality
/// between native codecs and the browser WebCodecs encoder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportVideoRatesV2 {
    pub video_bitrate: u32,
    pub audio_bitrate: Option<u32>,
}
pub fn classic_export_rates_v2(
    options: &ExportVideoOptionsV2,
    width: u32,
    height: u32,
) -> Option<ExportVideoRatesV2> {
    if width == 0 || height == 0 {
        return None;
    }
    let factor = match options.quality {
        ExportQualityV2::Low => 0.6,
        ExportQualityV2::Medium => 1.0,
        ExportQualityV2::High => 2.0,
        ExportQualityV2::VeryHigh => 4.0,
    };
    let efficiency = match options.format {
        ExportFormatV2::Mp4 => 1.0,
        ExportFormatV2::Webm => 0.6,
    };
    let video = (3_000_000.0
        * ((f64::from(width) * f64::from(height)) / (1920.0 * 1080.0)).powf(0.95)
        * efficiency
        * factor
        / 1000.0)
        .ceil()
        * 1000.0;
    if !video.is_finite() || video < 1.0 || video > f64::from(u32::MAX) {
        return None;
    }
    let audio = if !options.include_audio {
        None
    } else {
        Some(match options.format {
            ExportFormatV2::Mp4 => {
                let target = 128_000.0 * factor;
                // Strictly nearer keeps Classic's earlier entry on an equal tie.
                [128_000u32, 160_000, 192_000]
                    .into_iter()
                    .fold(96_000u32, |best, value| {
                        if (f64::from(value) - target).abs() < (f64::from(best) - target).abs() {
                            value
                        } else {
                            best
                        }
                    })
            }
            ExportFormatV2::Webm => ((64_000.0 * factor / 1000.0).round() * 1000.0) as u32,
        })
    };
    Some(ExportVideoRatesV2 {
        video_bitrate: video as u32,
        audio_bitrate: audio,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn classic_four_quality_targets_and_audio_switch_are_explicit() {
        for (quality, h264, vp9, aac, opus) in [
            (ExportQualityV2::Low, 1_800_000, 1_080_000, 96_000, 38_000),
            (
                ExportQualityV2::Medium,
                3_000_000,
                1_800_000,
                128_000,
                64_000,
            ),
            (
                ExportQualityV2::High,
                6_000_000,
                3_600_000,
                192_000,
                128_000,
            ),
            (
                ExportQualityV2::VeryHigh,
                12_000_000,
                7_200_000,
                192_000,
                256_000,
            ),
        ] {
            for (format, video, audio) in [
                (ExportFormatV2::Mp4, h264, aac),
                (ExportFormatV2::Webm, vp9, opus),
            ] {
                let mut options = ExportVideoOptionsV2 {
                    format,
                    quality,
                    include_audio: true,
                };
                assert_eq!(
                    classic_export_rates_v2(&options, 1920, 1080),
                    Some(ExportVideoRatesV2 {
                        video_bitrate: video,
                        audio_bitrate: Some(audio)
                    })
                );
                options.include_audio = false;
                assert_eq!(
                    classic_export_rates_v2(&options, 1920, 1080)
                        .unwrap()
                        .audio_bitrate,
                    None
                );
            }
        }
        assert!(classic_export_rates_v2(&ExportVideoOptionsV2::default(), 0, 1080).is_none());
        assert!(
            serde_json::from_value::<ExportVideoOptionsV2>(
                serde_json::json!({"format":"gif","quality":"high","include_audio":true})
            )
            .is_err()
        );
    }
}
