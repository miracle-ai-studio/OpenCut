use crate::MediaTime;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "SilenceRequest")]
pub struct SilenceRequest<P = String> {
    pub input: P,
    /// Threshold in dBFS, between -80 and -5.
    pub noise_db: i16,
    /// Minimum silent interval, between 100 and 10_000 milliseconds.
    pub min_duration_ms: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "ShotsRequest")]
pub struct ShotsRequest<P = String> {
    pub input: P,
    /// FFmpeg scene score threshold in [0.05, 0.95].
    pub threshold: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "WaveformRequest")]
pub struct WaveformRequest<P = String> {
    pub input: P,
    /// Maximum number of output buckets, from 16 through 2048.
    pub buckets: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SilenceInterval {
    pub start: MediaTime,
    pub end: MediaTime,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SilenceResult {
    pub intervals: Vec<SilenceInterval>,
    pub provider: String,
    pub method: String,
    pub confidence_known: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ShotBoundary {
    pub at: MediaTime,
    /// FFmpeg lavfi.scene_score; it is a frame difference score, not semantic confidence.
    pub scene_score: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ShotsResult {
    pub boundaries: Vec<ShotBoundary>,
    pub provider: String,
    pub method: String,
    pub confidence_known: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WaveformBucket {
    pub start: MediaTime,
    pub end: MediaTime,
    /// Linear amplitude 0..1 measured over this source interval.
    pub rms: f32,
    pub peak: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WaveformResult {
    pub buckets: Vec<WaveformBucket>,
    pub provider: String,
    pub method: String,
    pub confidence_known: bool,
}

pub fn validate_public_silence_result(value: &SilenceResult) -> bool {
    if value.provider != "ffmpeg"
        || value.method != "silencedetect"
        || value.confidence_known
        || value.intervals.len() > 10_000
    {
        return false;
    }
    let mut previous_end = 0;
    for interval in &value.intervals {
        if interval.start.timebase != 1000
            || interval.end.timebase != 1000
            || interval.start.ticks < previous_end
            || interval.end.ticks <= interval.start.ticks
            || interval.end.ticks > 3_600_000
        {
            return false;
        }
        previous_end = interval.end.ticks;
    }
    true
}

pub fn validate_public_shots_result(value: &ShotsResult) -> bool {
    if value.provider != "ffmpeg"
        || value.method != "scene_score"
        || value.confidence_known
        || value.boundaries.len() > 10_000
    {
        return false;
    }
    let mut previous = -1;
    for boundary in &value.boundaries {
        if boundary.at.timebase != 1000
            || boundary.at.ticks <= previous
            || boundary.at.ticks > 3_600_000
            || !boundary.scene_score.is_finite()
            || !(0.0..=1.0).contains(&boundary.scene_score)
        {
            return false;
        }
        previous = boundary.at.ticks;
    }
    true
}

pub fn validate_public_waveform_result(value: &WaveformResult) -> bool {
    if value.provider != "ffmpeg"
        || value.method != "astats_rms_peak"
        || value.confidence_known
        || value.buckets.len() > 2048
    {
        return false;
    }
    let mut previous_end = 0;
    for bucket in &value.buckets {
        if bucket.start.timebase != 16_000
            || bucket.end.timebase != 16_000
            || bucket.start.ticks < previous_end
            || bucket.end.ticks <= bucket.start.ticks
            || bucket.end.ticks > 57_600_000
            || !bucket.rms.is_finite()
            || !bucket.peak.is_finite()
            || !(0.0..=1.0).contains(&bucket.rms)
            || !(0.0..=1.0).contains(&bucket.peak)
            || bucket.rms > bucket.peak + 0.001
        {
            return false;
        }
        previous_end = bucket.end.ticks;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    fn time(ticks: i64, timebase: u32) -> MediaTime {
        MediaTime { ticks, timebase }
    }
    #[test]
    fn public_results_reject_corruption_and_provider_text() {
        let mut silence = SilenceResult {
            intervals: vec![SilenceInterval {
                start: time(0, 1000),
                end: time(100, 1000),
            }],
            provider: "ffmpeg".into(),
            method: "silencedetect".into(),
            confidence_known: false,
        };
        assert!(validate_public_silence_result(&silence));
        silence.intervals.push(SilenceInterval {
            start: time(50, 1000),
            end: time(200, 1000),
        });
        assert!(!validate_public_silence_result(&silence));
        silence.intervals.pop();
        silence.provider = "/private/ffmpeg".into();
        assert!(!validate_public_silence_result(&silence));

        let mut shots = ShotsResult {
            boundaries: vec![ShotBoundary {
                at: time(1000, 1000),
                scene_score: 0.5,
            }],
            provider: "ffmpeg".into(),
            method: "scene_score".into(),
            confidence_known: false,
        };
        assert!(validate_public_shots_result(&shots));
        shots.boundaries[0].scene_score = f32::NAN;
        assert!(!validate_public_shots_result(&shots));

        let mut waveform = WaveformResult {
            buckets: vec![WaveformBucket {
                start: time(0, 16_000),
                end: time(8, 16_000),
                rms: 0.1,
                peak: 0.3,
            }],
            provider: "ffmpeg".into(),
            method: "astats_rms_peak".into(),
            confidence_known: false,
        };
        assert!(validate_public_waveform_result(&waveform));
        waveform.buckets[0].peak = f32::INFINITY;
        assert!(!validate_public_waveform_result(&waveform));
    }
}
