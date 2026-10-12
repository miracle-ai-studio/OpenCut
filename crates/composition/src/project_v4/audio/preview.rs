//! Classic realtime facts: ordinary PCM preserves observed PTS; prepared PCM
//! starts at zero. Gain ramps interpolate physical amplitudes at 60 Hz from the
//! generation's anchor, rather than sampling dB independently at every sample.
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const CLASSIC_REALTIME_AUDIO_PROFILE_V4: &str = "classic-realtime-v31-web-audio";
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewGainOverrideV4 {
    pub track_id: String,
    pub element_id: String,
    pub decibels_tenths: i32,
}
struct GainTarget {
    track_id: String,
    locked: bool,
    decibels: bool,
    animated: bool,
}
pub struct PreparedPreviewAudioV4 {
    audio: PreparedAudioV4,
    targets: BTreeMap<String, GainTarget>,
}
impl PreparedPreviewAudioV4 {
    pub fn new(
        snapshot: &ProjectSnapshotV4,
        sequence_id: &str,
        format: PcmFormatV4,
    ) -> Result<Self, CompositionError> {
        prepare_preview_audio_v4(snapshot, sequence_id, format)
    }
    pub fn audio(&self) -> &PreparedAudioV4 {
        &self.audio
    }
    pub fn extent(&self) -> u64 {
        self.audio.duration_samples()
    }
    /// Use the same maximum admitted prefix as fixed-revision WAV execution.
    pub fn block_frame_limit(&self, start: u64) -> u32 {
        self.audio
            .block_frame_limit(start, MAX_AUDIO_BLOCK_FRAMES_V4)
    }
    pub fn validate_temporary_gain(
        &self,
        track_id: &str,
        element_id: &str,
        decibels_tenths: i32,
    ) -> Result<PreviewGainOverrideV4, CompositionError> {
        let target = self
            .targets
            .get(element_id)
            .filter(|t| t.track_id == track_id)
            .ok_or_else(|| {
                invalid("Temporary gain target does not belong to this fixed scene/track")
            })?;
        validate_gain_target(target, track_id, element_id, decibels_tenths)
    }
}
/// Validate one fixed element without preparing unrelated scene audio.
pub fn validate_preview_gain_override_v4(
    track: &TrackV4,
    element: &TimelineElementV4,
    decibels_tenths: i32,
) -> Result<PreviewGainOverrideV4, CompositionError> {
    if !track.elements.iter().any(|candidate| candidate == element) {
        return Err(invalid(
            "Temporary gain target does not belong to this fixed scene/track",
        ));
    }
    let style = match &element.content {
        ElementContentV4::Audio { audio, .. } | ElementContentV4::Video { audio, .. } => audio,
        _ => return Err(invalid("Temporary gain requires an audio-bearing element")),
    };
    let target = GainTarget {
        track_id: track.id.clone(),
        locked: track.locked,
        decibels: matches!(style.gain, GainV4::Decibels { .. }),
        animated: has_keys(&gains::channels(&element.channels)),
    };
    validate_gain_target(&target, &track.id, &element.id, decibels_tenths)
}
fn validate_gain_target(
    target: &GainTarget,
    track_id: &str,
    element_id: &str,
    decibels_tenths: i32,
) -> Result<PreviewGainOverrideV4, CompositionError> {
    if target.locked {
        return Err(CompositionError {
            code: "LOCKED_TRACK",
            message: "Temporary gain target is on a locked track".into(),
        });
    }
    if !target.decibels {
        return Err(unsupported(
            "Temporary gain cannot reinterpret legacy linear gain",
        ));
    }
    if target.animated {
        return Err(unsupported(
            "Temporary static gain cannot replace an animated volume envelope",
        ));
    }
    if !(-600..=200).contains(&decibels_tenths) {
        return Err(invalid("Temporary gain requires -600..200 tenths dB"));
    }
    Ok(PreviewGainOverrideV4 {
        track_id: track_id.into(),
        element_id: element_id.into(),
        decibels_tenths,
    })
}
fn has_keys(channels: &[AnimationChannelV4]) -> bool {
    channels
        .iter()
        .any(|c| matches!(c, AnimationChannelV4::Scalar { keys, .. } if !keys.is_empty()))
}
pub fn prepare_preview_audio_v4(
    snapshot: &ProjectSnapshotV4,
    sequence_id: &str,
    format: PcmFormatV4,
) -> Result<PreparedPreviewAudioV4, CompositionError> {
    if !matches!(format.sample_rate, 44_100 | 48_000) || format.channels != 2 {
        return Err(unsupported(
            "Realtime browser audio requires 44100 or 48000 Hz stereo",
        ));
    }
    let mut audio = prepare_audio_v4(
        snapshot,
        sequence_id,
        AudioExecutionProfileV4::ClassicV31,
        format,
    )?;
    let sequence = snapshot
        .project
        .sequences
        .iter()
        .find(|s| s.id == sequence_id)
        .ok_or_else(|| invalid("Missing fixed preview scene"))?;
    let mut targets = BTreeMap::new();
    for track in &sequence.tracks {
        for element in &track.elements {
            let style = match &element.content {
                ElementContentV4::Audio { audio, .. } | ElementContentV4::Video { audio, .. } => {
                    audio
                }
                _ => continue,
            };
            targets.insert(
                element.id.clone(),
                GainTarget {
                    track_id: track.id.clone(),
                    locked: track.locked,
                    decibels: matches!(style.gain, GainV4::Decibels { .. }),
                    animated: has_keys(&gains::channels(&element.channels)),
                },
            );
        }
    }
    for voice in &mut audio.voices {
        if (has_keys(&voice.element_channels)
            && matches!(voice.element_gain, GainV4::LinearLegacy { .. }))
            || (has_keys(&voice.track_channels)
                && matches!(voice.track_gain, GainV4::LinearLegacy { .. }))
        {
            return Err(unsupported(
                "Realtime 60 Hz ramps do not reinterpret animated legacy linear gain",
            ));
        }
        // Classic uses a concatenated, retimed Float32 buffer for animated
        // element volume even when maintainPitch is false. Track animation is
        // an explicit V4 extension and keeps its independent sequence clock.
        if has_keys(&voice.element_channels)
            && voice.processing == AudioProcessingV4::LinearInterpolationZeroTail
        {
            voice.processing = AudioProcessingV4::ClassicPreparedLinear;
        }
    }
    budget::admit(&audio.voices)?;
    Ok(PreparedPreviewAudioV4 { audio, targets })
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewAudioSegmentV4 {
    pub segment_id: u32,
    /// Pinned decoder's observed seconds, not authored project ticks.
    pub timestamp_seconds: f64,
    #[serde(with = "runtime_pcm::decimal_u64")]
    pub frame_count: u64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewSourceFormatV4 {
    pub element_id: String,
    pub sample_rate: u32,
    #[serde(with = "runtime_pcm::decimal_u64")]
    pub frame_count: u64,
    pub channels: u32,
    pub prepared: bool,
    /// Ordinary sources retain native-rate chunks and their PTS. Prepared
    /// sources contain one target-rate buffer and must supply an empty list.
    pub segments: Vec<PreviewAudioSegmentV4>,
}
pub struct BoundPreviewAudioV4<'a> {
    prepared: &'a PreparedPreviewAudioV4,
    formats: Vec<BoundPreviewSourceV4>,
}
struct BoundPreviewSourceV4 {
    format: PreviewSourceFormatV4,
    native_starts: Vec<i64>,
}

// The pinned WebCodecs decoder rounds PTS to native samples, but its PCM
// decoder accumulates up to 65,536 packet durations in f64 without that round.
// Allow only bounded arithmetic/JSON rounding around an unambiguous sample
// index. Keep the original PTS in the binding and sampling facts; this check
// must neither shift a real gap nor turn arbitrary off-grid PTS into support.
const MAX_OBSERVED_NATIVE_SAMPLE: i64 = 9_007_199_254_740_991;
fn observed_native_sample(
    timestamp: f64,
    sample_rate: u32,
    scale: f64,
) -> Result<i64, CompositionError> {
    let rate = f64::from(sample_rate);
    let rounded = (timestamp * rate).round();
    if !timestamp.is_finite()
        || !rounded.is_finite()
        || rounded.abs() > MAX_OBSERVED_NATIVE_SAMPLE as f64
    {
        return Err(invalid(
            "Observed PTS exceeds the exact signed native sample range",
        ));
    }
    let index = rounded as i64;
    let observed = timestamp * rate;
    let rounding_bound = (f64::EPSILON * 131_080.0 * scale.max(1.0)).min(1.0 / 1024.0);
    if (observed - rounded).abs() > rounding_bound
        || (index - 1) as f64 / rate == timestamp
        || (index + 1) as f64 / rate == timestamp
    {
        return Err(invalid(
            "Observed PTS is not an unambiguous pinned native sample grid point",
        ));
    }
    Ok(index)
}

pub fn bind_preview_audio_v4<'a>(
    prepared: &'a PreparedPreviewAudioV4,
    formats: &[PreviewSourceFormatV4],
) -> Result<BoundPreviewAudioV4<'a>, CompositionError> {
    if formats.len() != prepared.audio.voices.len() {
        return Err(invalid(
            "Preview sources must bind the complete fixed voice set",
        ));
    }
    let mut by_id = BTreeMap::new();
    for f in formats {
        if by_id.insert(f.element_id.as_str(), f).is_some() {
            return Err(invalid("Duplicate preview source"));
        }
    }
    let mut output = Vec::with_capacity(formats.len());
    for voice in &prepared.audio.voices {
        let f = by_id
            .remove(voice.element_id.as_str())
            .ok_or_else(|| invalid("Unknown or missing preview source"))?;
        if f.sample_rate == 0
            || f.sample_rate > 384_000
            || f.frame_count == 0
            || !(1..=2).contains(&f.channels)
        {
            return Err(invalid("Invalid actual preview PCM format"));
        }
        let is_prepared = voice.processing != AudioProcessingV4::LinearInterpolationZeroTail;
        if f.prepared != is_prepared {
            return Err(invalid("Preview source mode differs from its fixed voice"));
        }
        let mut native_starts = Vec::new();
        if is_prepared {
            let frames = clocks::sample_index(
                time(&voice.timeline_duration)?,
                prepared.audio.identity.format.sample_rate,
                clocks::Rounding::Ceil,
            )?
            .max(1);
            if !f.segments.is_empty()
                || f.sample_rate != prepared.audio.identity.format.sample_rate
                || f.frame_count != frames
            {
                return Err(invalid(
                    "Prepared preview PCM must retain exact output length, target rate and zero origin",
                ));
            }
        } else {
            if f.segments.is_empty() || f.segments.len() > 65_536 {
                return Err(invalid(
                    "Ordinary preview PCM requires a bounded PTS segment list",
                ));
            }
            let mut ids = std::collections::BTreeSet::new();
            let mut count = 0u64;
            let mut last_end = None;
            // Use the complete observed range for cancellation around zero.
            // Two rounding operations per admitted packet plus transport,
            // capped at 1/1024 of a native sample, cannot hide a sample gap.
            let scale = f
                .segments
                .iter()
                .map(|s| s.timestamp_seconds.abs() * f64::from(f.sample_rate))
                .fold(1.0, f64::max);
            for s in &f.segments {
                if !ids.insert(s.segment_id) || s.frame_count == 0 {
                    return Err(invalid("Invalid observed PTS segment"));
                }
                let start = observed_native_sample(s.timestamp_seconds, f.sample_rate, scale)?;
                let length = i64::try_from(s.frame_count)
                    .map_err(|_| invalid("Decoded PTS segment frame count overflow"))?;
                let end = start
                    .checked_add(length)
                    .filter(|end| {
                        (-MAX_OBSERVED_NATIVE_SAMPLE..=MAX_OBSERVED_NATIVE_SAMPLE).contains(end)
                    })
                    .ok_or_else(|| invalid("Decoded PTS segment endpoint overflow"))?;
                if last_end.is_some_and(|previous| start < previous) {
                    return Err(unsupported(
                        "Overlapping or out-of-order decoded PTS segments require an explicit decoder profile",
                    ));
                }
                last_end = Some(end);
                native_starts.push(start);
                count = count
                    .checked_add(s.frame_count)
                    .ok_or_else(|| invalid("Decoded segment frame count overflow"))?;
            }
            if count != f.frame_count {
                return Err(invalid(
                    "Decoded segments do not cover the actual PCM frame count",
                ));
            }
        }
        output.push(BoundPreviewSourceV4 {
            format: f.clone(),
            native_starts,
        });
    }
    Ok(BoundPreviewAudioV4 {
        prepared,
        formats: output,
    })
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewSourceSampleV4 {
    pub segment_id: u32,
    #[serde(serialize_with = "runtime_pcm::serialize_u64")]
    pub lower: u64,
    #[serde(serialize_with = "runtime_pcm::serialize_u64")]
    pub upper: u64,
    pub fraction: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewAudioVoiceBlockV4 {
    pub element_id: String,
    pub block_offset: u32,
    pub frame_count: u32,
    pub positions: Vec<Option<PreviewSourceSampleV4>>,
    pub amplitudes: Vec<f64>,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewAudioIdentityV4 {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub profile: &'static str,
    pub sample_rate: u32,
    pub channels: u32,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewAudioBlockV4 {
    pub identity: PreviewAudioIdentityV4,
    #[serde(serialize_with = "runtime_pcm::serialize_u64")]
    pub start_sample: u64,
    pub frame_count: u32,
    pub voices: Vec<PreviewAudioVoiceBlockV4>,
    pub receipts: Vec<AudioReceiptV4>,
}
fn preview_amplitude(
    base: GainV4,
    channels: &[AnimationChannelV4],
    seconds: f64,
) -> Result<f64, CompositionError> {
    let ticks = (seconds * 120_000.0).round();
    if !ticks.is_finite() || ticks < 0.0 || ticks >= i64::MAX as f64 {
        return Err(invalid("Realtime gain clock overflows"));
    }
    let at: Time = RationalTime::new(ticks as i64, 120_000)
        .map_err(error)?
        .into();
    match gains::physical(base, channels, Some(&at))? {
        GainV4::Decibels { value } => finite(10f64.powf(value.clamp(-60.0, 20.0) / 20.0)),
        linear => Ok(animation::gain(linear)?.amplitude),
    }
}
fn ramp(
    base: GainV4,
    channels: &[AnimationChannelV4],
    sample: u64,
    anchor: u64,
    end_seconds: f64,
    rate: u32,
) -> Result<f64, CompositionError> {
    if !has_keys(channels) {
        return preview_amplitude(base, &[], 0.0);
    }
    let seconds = sample as f64 / f64::from(rate);
    let origin = anchor as f64 / f64::from(rate);
    let offset = (seconds - origin).max(0.0);
    let knot = (offset * 60.0).floor();
    let left = (origin + knot / 60.0).min(end_seconds);
    let right = (origin + (knot + 1.0) / 60.0).min(end_seconds);
    let a = preview_amplitude(base, channels, left)?;
    if right <= left {
        return Ok(a);
    }
    let b = preview_amplitude(base, channels, right)?;
    finite(a + (b - a) * ((seconds - left) / (right - left)).clamp(0.0, 1.0))
}
pub fn compile_preview_audio_block_v4(
    bound: &BoundPreviewAudioV4<'_>,
    start_sample: u64,
    frame_count: u32,
    anchor_sample: u64,
    gain: Option<&PreviewGainOverrideV4>,
) -> Result<PreviewAudioBlockV4, CompositionError> {
    if anchor_sample >= bound.prepared.extent()
        || start_sample < anchor_sample
        || start_sample
            .checked_add(u64::from(frame_count))
            .is_none_or(|end| end > bound.prepared.extent())
    {
        return Err(invalid("Preview block precedes its generation anchor"));
    }
    if let Some(g) = gain {
        bound
            .prepared
            .validate_temporary_gain(&g.track_id, &g.element_id, g.decibels_tenths)?;
    }
    let audio = &bound.prepared.audio;
    let block = compile_audio_block_inner(audio, start_sample, frame_count, false)?;
    let rate = audio.identity.format.sample_rate;
    let mut voices = Vec::with_capacity(block.voices.len());
    for facts in block.voices {
        let voice = audio
            .voices
            .iter()
            .find(|v| v.element_id == facts.element_id)
            .expect("compiled fixed voice");
        let format = bound
            .formats
            .iter()
            .find(|f| f.format.element_id == facts.element_id)
            .expect("complete binding");
        let source = format;
        let format = &source.format;
        let mut positions = Vec::with_capacity(facts.frame_count as usize);
        let mut amplitudes = Vec::with_capacity(facts.frame_count as usize);
        let local_anchor = anchor_sample.saturating_sub(voice.start);
        let duration = time(&voice.timeline_duration)?;
        let end_seconds = duration.ticks() as f64 / f64::from(duration.timebase());
        for offset in 0..u64::from(facts.frame_count) {
            let local = facts.local_start_sample + offset;
            let position = if format.prepared {
                (local < format.frame_count).then_some(PreviewSourceSampleV4 {
                    segment_id: 0,
                    lower: local,
                    upper: (local + 1).min(format.frame_count - 1),
                    fraction: 0.0,
                })
            } else {
                let exact = voice.source.position_at(local, format.sample_rate)?;
                let native = i128::from(exact.lower);
                let index = source
                    .native_starts
                    .partition_point(|s| i128::from(*s) <= native);
                index
                    .checked_sub(1)
                    .and_then(|i| format.segments.get(i).map(|segment| (i, segment)))
                    .and_then(|(i, segment)| {
                        let local = native - i128::from(source.native_starts[i]);
                        if local < 0 || local >= i128::from(segment.frame_count) {
                            return None;
                        }
                        let lower = local as u64;
                        Some(PreviewSourceSampleV4 {
                            segment_id: segment.segment_id,
                            lower,
                            upper: (lower + 1).min(segment.frame_count - 1),
                            fraction: exact.fraction_numerator as f64
                                / exact.fraction_denominator as f64,
                        })
                    })
            };
            positions.push(position);
            let element_gain =
                gain.filter(|g| g.element_id == voice.element_id)
                    .map_or(voice.element_gain, |g| GainV4::Decibels {
                        value: f64::from(g.decibels_tenths) / 10.0,
                    });
            let element = ramp(
                element_gain,
                &voice.element_channels,
                local,
                local_anchor,
                end_seconds,
                rate,
            )?;
            let sequence_sample = start_sample + u64::from(facts.block_offset) + offset;
            let track = ramp(
                voice.track_gain,
                &voice.track_channels,
                sequence_sample,
                anchor_sample,
                audio.duration_samples as f64 / f64::from(rate),
                rate,
            )?;
            amplitudes.push(finite(element * track)?);
        }
        voices.push(PreviewAudioVoiceBlockV4 {
            element_id: facts.element_id,
            block_offset: facts.block_offset,
            frame_count: facts.frame_count,
            positions,
            amplitudes,
        });
    }
    Ok(PreviewAudioBlockV4 {
        identity: PreviewAudioIdentityV4 {
            project_id: audio.identity.project_id.clone(),
            revision: audio.identity.revision.clone(),
            sequence_id: audio.identity.sequence_id.clone(),
            profile: CLASSIC_REALTIME_AUDIO_PROFILE_V4,
            sample_rate: rate,
            channels: 2,
        },
        start_sample,
        frame_count,
        voices,
        receipts: block.receipts,
    })
}
