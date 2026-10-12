//! Bind actual final PCM buffers to one immutable preparation. This module
//! contains sample facts only; decoding, DSP, PCM reads and policy stay in hosts.
use super::*;
use serde::{Deserialize, Serialize, Serializer, ser::SerializeStruct};

/// Actual final buffer format, after any Classic pitch preparation. This is an
/// internal execution port, not a frozen project/public protocol declaration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeSourceFormatV4 {
    pub element_id: String,
    pub sample_rate: u32,
    #[serde(with = "decimal_u64")]
    pub frame_count: u64,
    pub channels: u32,
    pub mode: ClassicAudioPreparationModeV4,
}

/// A private complete binding in stable prepared voice order. Borrowing keeps
/// all source/gain clocks and the fixed revision owned by the original plan.
pub struct BoundAudioPcmV4<'a> {
    prepared: &'a PreparedAudioV4,
    formats: Vec<RuntimeSourceFormatV4>,
}
impl BoundAudioPcmV4<'_> {
    pub fn identity(&self) -> &AudioIdentityV4 {
        self.prepared.identity()
    }
    pub fn duration_samples(&self) -> u64 {
        self.prepared.duration_samples()
    }
    pub fn receipts(&self) -> &[AudioReceiptV4] {
        self.prepared.receipts()
    }
    pub fn source_formats(&self) -> &[RuntimeSourceFormatV4] {
        &self.formats
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeAudioVoiceBlockV4 {
    pub element_id: String,
    pub block_offset: u32,
    pub frame_count: u32,
    /// Exact indices remain nominal even beyond the actual PCM length. The
    /// renderer reads a zero tail and clamps only the last upper interpolation
    /// tap, as Classic does; these facts never authorize an out-of-buffer read.
    pub positions: Vec<ExactSourceSampleV4>,
    /// Physical amplitudes; gain clocks never switch to a prepared source clock.
    pub amplitudes: Vec<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeAudioBlockV4 {
    #[serde(serialize_with = "serialize_identity")]
    pub identity: AudioIdentityV4,
    #[serde(serialize_with = "serialize_u64")]
    pub start_sample: u64,
    pub frame_count: u32,
    pub voices: Vec<RuntimeAudioVoiceBlockV4>,
    /// Original participation and per-block OutsideBlock decisions, unchanged
    /// by runtime format binding; no resource locator is exposed.
    pub receipts: Vec<AudioReceiptV4>,
}

fn classic(prepared: &PreparedAudioV4) -> Result<(), CompositionError> {
    if prepared.identity.profile != AudioExecutionProfileV4::ClassicV31 {
        return Err(unsupported(
            "Runtime PCM binding requires the Classic browser profile",
        ));
    }
    budget::admit(&prepared.voices)
}
fn expected_mode(
    processing: AudioProcessingV4,
) -> Result<ClassicAudioPreparationModeV4, CompositionError> {
    match processing {
        AudioProcessingV4::LinearInterpolationZeroTail => {
            Ok(ClassicAudioPreparationModeV4::UnpreparedLinear)
        }
        AudioProcessingV4::ClassicPreparedLinear => {
            Ok(ClassicAudioPreparationModeV4::PreparedLinear)
        }
        AudioProcessingV4::NativeMaintainPitch => {
            Ok(ClassicAudioPreparationModeV4::PreparedSoundtouch)
        }
        AudioProcessingV4::LegacyFfmpegResample => Err(unsupported(
            "Legacy FFmpeg PCM is not a Classic prepared buffer",
        )),
    }
}
fn format_valid(format: &RuntimeSourceFormatV4) -> Result<(), CompositionError> {
    if format.element_id.is_empty()
        || format.sample_rate == 0
        || format.sample_rate > 384_000
        || format.frame_count == 0
        || !(1..=64).contains(&format.channels)
    {
        return Err(invalid(
            "Actual runtime PCM requires an element ID, positive length, 1..384000 Hz and 1..64 channels",
        ));
    }
    Ok(())
}

pub fn bind_classic_audio_pcm_v4<'a>(
    prepared: &'a PreparedAudioV4,
    formats: &[RuntimeSourceFormatV4],
) -> Result<BoundAudioPcmV4<'a>, CompositionError> {
    classic(prepared)?;
    if formats.len() != prepared.voices.len() {
        return Err(invalid(
            "Runtime PCM must bind every participating voice exactly once",
        ));
    }
    let mut by_id = HashMap::with_capacity(formats.len());
    for format in formats {
        format_valid(format)?;
        if by_id.insert(format.element_id.as_str(), format).is_some() {
            return Err(invalid("Runtime PCM contains a duplicate element ID"));
        }
    }
    let mut bound = Vec::with_capacity(prepared.voices.len());
    for voice in &prepared.voices {
        let format = by_id.remove(voice.element_id.as_str()).ok_or_else(|| {
            invalid("Runtime PCM contains an unknown element or omits a participating voice")
        })?;
        let expected = expected_mode(voice.processing)?;
        if format.mode != expected {
            return Err(invalid(
                "Runtime PCM preparation mode does not match its fixed voice",
            ));
        }
        if expected != ClassicAudioPreparationModeV4::UnpreparedLinear {
            let authored_frames = clocks::sample_index(
                time(&voice.timeline_duration)?,
                prepared.identity.format.sample_rate,
                clocks::Rounding::Ceil,
            )?
            .max(1);
            if format.sample_rate != prepared.identity.format.sample_rate
                || format.frame_count != authored_frames
                || format.channels > 2
            {
                return Err(invalid(
                    "Prepared PCM requires target sample rate, exact authored output length and mono/stereo",
                ));
            }
        }
        bound.push(format.clone());
    }
    Ok(BoundAudioPcmV4 {
        prepared,
        formats: bound,
    })
}

fn interval(start: u64, count: u32) -> Result<u64, CompositionError> {
    if count == 0 || count > MAX_AUDIO_BLOCK_FRAMES_V4 {
        return Err(invalid("Runtime audio block must contain 1..4096 frames"));
    }
    start
        .checked_add(u64::from(count))
        .ok_or_else(|| invalid("Runtime audio sample boundary overflows u64"))
}

/// Original mapping for prepared-linear's intermediate Float32 write. This
/// intentionally runs before final buffer binding, so there is no preparation
/// dependency cycle. Position generation does not claim decoded samples exist.
pub fn compile_original_audio_positions_v4(
    prepared: &PreparedAudioV4,
    element_id: &str,
    local_start_sample: u64,
    frame_count: u32,
    decoded_sample_rate: u32,
) -> Result<Vec<ExactSourceSampleV4>, CompositionError> {
    classic(prepared)?;
    let end = interval(local_start_sample, frame_count)?;
    if decoded_sample_rate == 0 || decoded_sample_rate > 384_000 {
        return Err(invalid(
            "Actual decoded PCM sample rate is outside 1..384000 Hz",
        ));
    }
    let voice = prepared
        .voices
        .iter()
        .find(|v| v.element_id == element_id)
        .ok_or_else(|| invalid("Original source positions require a participating element"))?;
    if end > voice.end - voice.start {
        return Err(invalid(
            "Original source positions exceed the authored output span",
        ));
    }
    (local_start_sample..end)
        .map(|sample| voice.source.position_at(sample, decoded_sample_rate))
        .collect()
}

/// Dense executable facts, retaining the authored gain clocks and original
/// revision. Prepared PCM advances from zero at unit rate; original PCM uses
/// the exact effective source rate and trim at its real decoded sample rate.
pub fn compile_bound_audio_block_v4(
    bound: &BoundAudioPcmV4<'_>,
    start_sample: u64,
    frame_count: u32,
) -> Result<RuntimeAudioBlockV4, CompositionError> {
    interval(start_sample, frame_count)?;
    let block = compile_audio_block_v4(bound.prepared, start_sample, frame_count)?;
    let mut voices = Vec::with_capacity(block.voices.len());
    for voice in block.voices {
        let format = bound
            .formats
            .iter()
            .find(|f| f.element_id == voice.element_id)
            .ok_or_else(|| invalid("Validated runtime PCM binding is incomplete"))?;
        let prepared_source = format.mode != ClassicAudioPreparationModeV4::UnpreparedLinear;
        let end = voice
            .local_start_sample
            .checked_add(u64::from(voice.frame_count))
            .ok_or_else(|| invalid("Runtime voice source boundary overflow"))?;
        let positions = (voice.local_start_sample..end)
            .map(|sample| {
                if prepared_source {
                    Ok(ExactSourceSampleV4 {
                        lower: sample,
                        fraction_numerator: 0,
                        fraction_denominator: 1,
                    })
                } else {
                    voice.source.position_at(sample, format.sample_rate)
                }
            })
            .collect::<Result<Vec<_>, CompositionError>>()?;
        let amplitudes = match voice.gain {
            SampleGainV4::Constant { amplitude } => vec![amplitude; voice.frame_count as usize],
            SampleGainV4::Dense { amplitudes } => amplitudes,
        };
        voices.push(RuntimeAudioVoiceBlockV4 {
            element_id: voice.element_id,
            block_offset: voice.block_offset,
            frame_count: voice.frame_count,
            positions,
            amplitudes,
        });
    }
    Ok(RuntimeAudioBlockV4 {
        identity: block.identity,
        start_sample: block.start_sample,
        frame_count: block.frame_count,
        voices,
        receipts: block.receipts,
    })
}

pub(super) fn serialize_u64<S: Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&value.to_string())
}
pub(super) fn serialize_u128<S: Serializer>(
    value: &u128,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&value.to_string())
}
fn serialize_identity<S: Serializer>(
    value: &AudioIdentityV4,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let mut identity = serializer.serialize_struct("AudioIdentityV4", 6)?;
    identity.serialize_field("projectId", &value.project_id)?;
    identity.serialize_field("revision", &value.revision)?;
    identity.serialize_field("sequenceId", &value.sequence_id)?;
    identity.serialize_field(
        "profile",
        &match value.profile {
            AudioExecutionProfileV4::ClassicV31 => "classic-export-v31-web-audio",
            AudioExecutionProfileV4::LegacyFfmpegV3 => "legacy-ffmpeg-v3",
        },
    )?;
    identity.serialize_field("sampleRate", &value.format.sample_rate)?;
    identity.serialize_field("channels", &value.format.channels)?;
    identity.end()
}
pub(super) mod decimal_u64 {
    use serde::{Deserialize, Deserializer, Serializer, de::Error};
    pub(in crate::project_v4::audio) fn serialize<S: Serializer>(
        value: &u64,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        super::serialize_u64(value, serializer)
    }
    pub(in crate::project_v4::audio) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<u64, D::Error> {
        let value = String::deserialize(deserializer)?;
        if value.is_empty()
            || (value.len() > 1 && value.starts_with('0'))
            || !value.bytes().all(|c| c.is_ascii_digit())
        {
            return Err(D::Error::custom(
                "Expected an unsigned canonical decimal string",
            ));
        }
        value.parse().map_err(D::Error::custom)
    }
}
