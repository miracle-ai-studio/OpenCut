//! Exact Classic audio-buffer preparation facts. No decoder, PCM, allocator,
//! native pitch library or authored-project mutation is owned by this module.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClassicAudioPreparationMode {
    UnpreparedLinear,
    PreparedLinear,
    PreparedSoundtouch,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClassicAudioPreparationInput {
    /// Source request trimStart, before conversion to native sample indices.
    pub source_start: Time,
    /// Actual authored timeline duration, never ceil(placement frames)/rate.
    pub timeline_duration: Time,
    #[serde(with = "rational_decimal")]
    pub rate: RationalRate,
    pub maintain_pitch: bool,
    pub decoded_sample_rate: u32,
    #[serde(with = "decimal")]
    pub decoded_frames: u64,
    pub decoded_channels: u32,
    pub target_sample_rate: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClassicAudioPreparationBudget {
    /// Actual sliced/resampled buffers allocated by SoundTouch preparation.
    #[serde(with = "decimal")]
    pub max_source_frames: u64,
    #[serde(with = "decimal")]
    pub max_output_frames: u64,
    /// Resident decoded channels plus buffers actually allocated by the mode. Native library
    /// FIFO and decoder temporary work memory need separate adapter admission.
    #[serde(with = "decimal")]
    pub max_total_channel_frames: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClassicAudioPreparationFacts {
    pub mode: ClassicAudioPreparationMode,
    #[serde(with = "decimal")]
    pub source_slice_start_sample: u64,
    #[serde(with = "decimal")]
    pub requested_source_frames: u64,
    #[serde(with = "decimal")]
    pub actual_source_frames: u64,
    #[serde(with = "decimal")]
    pub resampled_frames: u64,
    #[serde(with = "decimal")]
    pub output_frames: u64,
    pub num_channels: u32,
    pub decoded_sample_rate: u32,
    pub target_sample_rate: u32,
    #[serde(with = "rational_decimal")]
    pub tempo: RationalRate,
    #[serde(with = "decimal")]
    pub prefix_frames: u64,
    #[serde(with = "decimal")]
    pub total_channel_frames: u64,
    /// Actual decoded buffer channels are counted even when only its first two
    /// participate in the Classic prepared buffer.
    #[serde(with = "decimal")]
    pub decoded_bytes: u64,
    /// Direct sampling allocates none; prepared-linear allocates its output;
    /// SoundTouch also allocates the sliced and resampled Float32 buffers.
    #[serde(with = "decimal")]
    pub prepared_bytes: u64,
    #[serde(with = "decimal")]
    pub total_bytes: u64,
}

/// Classic clampRetimeRate: preserve authored rate separately, evaluate at
/// [1/100, 5]. The reduced rational is the sole effective sample-clock tempo.
pub fn classic_effective_rate_scene(raw: RationalRate) -> Result<RationalRate, CompositionError> {
    if raw.numerator == 0 || raw.denominator == 0 {
        return Err(invalid("Classic audio rate must be positive"));
    }
    if u64::from(raw.numerator) * 100 < u64::from(raw.denominator) {
        return Ok(RationalRate {
            numerator: 1,
            denominator: 100,
        });
    }
    if u64::from(raw.numerator) > u64::from(raw.denominator) * 5 {
        return Ok(RationalRate {
            numerator: 5,
            denominator: 1,
        });
    }
    let (mut a, mut b) = (raw.numerator, raw.denominator);
    while b != 0 {
        (a, b) = (b, a % b);
    }
    Ok(RationalRate {
        numerator: raw.numerator / a,
        denominator: raw.denominator / a,
    })
}

/// Export prepares a Float32 buffer whenever keepPitch is true, including
/// identity and epsilon-close tempos. Only abs(effective tempo-1)>1e-6 invokes
/// soundtouch. This classification does not prove that processor is available.
pub fn classic_pitch_processing_mode_scene(
    raw: RationalRate,
    maintain_pitch: bool,
) -> Result<ClassicAudioPreparationMode, CompositionError> {
    let rate = classic_effective_rate_scene(raw)?;
    if !maintain_pitch {
        return Ok(ClassicAudioPreparationMode::UnpreparedLinear);
    }
    // Classic selects its DSP branch with IEEE-754 Number subtraction. This
    // intentionally differs from an exact rational epsilon comparison just
    // below one: 999999/1000000 has abs(rate-1) slightly greater than 1e-6.
    // Source positions and buffer counts remain exact rational calculations.
    let effective = f64::from(rate.numerator) / f64::from(rate.denominator);
    Ok(if (effective - 1.0).abs() > 1e-6 {
        ClassicAudioPreparationMode::PreparedSoundtouch
    } else {
        ClassicAudioPreparationMode::PreparedLinear
    })
}

fn count(value: u128) -> Result<u64, CompositionError> {
    u64::try_from(value).map_err(|_| invalid("Classic audio preparation count overflows u64"))
}
fn scaled_frames(
    duration: RationalTime,
    rate: RationalRate,
    sample_rate: u32,
) -> Result<u64, CompositionError> {
    let top = u128::from(duration.ticks().unsigned_abs())
        .checked_mul(u128::from(rate.numerator))
        .and_then(|n| n.checked_mul(u128::from(sample_rate)))
        .ok_or_else(|| invalid("Classic audio preparation numerator overflow"))?;
    let bottom = u128::from(duration.timebase())
        .checked_mul(u128::from(rate.denominator))
        .ok_or_else(|| invalid("Classic audio preparation denominator overflow"))?;
    if bottom == 0 {
        return Err(invalid("Classic audio preparation denominator is zero"));
    }
    count(top.div_ceil(bottom).max(1))
}

pub fn classic_audio_preparation_facts_scene(
    input: &ClassicAudioPreparationInput,
    budget: &ClassicAudioPreparationBudget,
) -> Result<ClassicAudioPreparationFacts, CompositionError> {
    preparation_facts(input, budget, None, None)
}

/// Sparse source keeps the original logical source/address math, but counts
/// only resident decoded PCM in allocation facts. No global prefix is allocated.
pub fn classic_audio_preparation_windowed(
    input: &ClassicAudioPreparationInput,
    budget: &ClassicAudioPreparationBudget,
    resident_frames: u64,
) -> Result<ClassicAudioPreparationFacts, CompositionError> {
    if resident_frames > input.decoded_frames {
        return Err(invalid("Resident PCM exceeds the logical decoded source"));
    }
    preparation_facts(input, budget, Some(resident_frames), None)
}

/// The compiled preview may require a separate Float32 preparation stage for
/// animated volume even without pitch preservation. Select that actual mode
/// before allocation admission, rather than changing the facts afterwards.
pub fn classic_audio_preparation_for_mode(
    input: &ClassicAudioPreparationInput,
    budget: &ClassicAudioPreparationBudget,
    resident_frames: Option<u64>,
    mode: ClassicAudioPreparationMode,
) -> Result<ClassicAudioPreparationFacts, CompositionError> {
    if resident_frames.is_some_and(|frames| frames > input.decoded_frames) {
        return Err(invalid("Resident PCM exceeds the logical decoded source"));
    }
    preparation_facts(input, budget, resident_frames, Some(mode))
}
fn preparation_facts(
    input: &ClassicAudioPreparationInput,
    budget: &ClassicAudioPreparationBudget,
    resident_frames: Option<u64>,
    selected_mode: Option<ClassicAudioPreparationMode>,
) -> Result<ClassicAudioPreparationFacts, CompositionError> {
    if input.decoded_sample_rate == 0
        || input.target_sample_rate == 0
        || input.decoded_channels == 0
    {
        return Err(invalid(
            "Classic audio preparation needs positive decoded/target rates and channels",
        ));
    }
    let start = RationalTime::try_from(&input.source_start).map_err(error)?;
    let duration = RationalTime::try_from(&input.timeline_duration).map_err(error)?;
    if duration.ticks() <= 0 {
        return Err(invalid(
            "Classic audio preparation needs positive actual timeline duration",
        ));
    }
    let tempo = classic_effective_rate_scene(input.rate)?;
    let expected_mode = classic_pitch_processing_mode_scene(tempo, input.maintain_pitch)?;
    let mode = selected_mode.unwrap_or(expected_mode);
    if mode != expected_mode
        && !(expected_mode == ClassicAudioPreparationMode::UnpreparedLinear
            && mode == ClassicAudioPreparationMode::PreparedLinear)
    {
        return Err(invalid(
            "Compiled preparation mode differs from the source processing",
        ));
    }
    let source_slice_start_sample = if start.ticks() <= 0 {
        0
    } else {
        count(
            u128::from(start.ticks().unsigned_abs()) * u128::from(input.decoded_sample_rate)
                / u128::from(start.timebase()),
        )?
    };
    let requested_source_frames = scaled_frames(duration, tempo, input.decoded_sample_rate)?;
    let available = input
        .decoded_frames
        .saturating_sub(source_slice_start_sample);
    let actual_source_frames = requested_source_frames.min(available).max(1);
    let resampled_frames = scaled_frames(duration, tempo, input.target_sample_rate)?;
    let output_frames = scaled_frames(
        duration,
        RationalRate {
            numerator: 1,
            denominator: 1,
        },
        input.target_sample_rate,
    )?;
    let num_channels = input.decoded_channels.min(2);
    let prefix_frames = source_slice_start_sample
        .checked_add(requested_source_frames)
        .ok_or_else(|| invalid("Classic audio source prefix overflow"))?;
    let decoded_channel_frames = u128::from(resident_frames.unwrap_or(input.decoded_frames))
        * u128::from(input.decoded_channels);
    let prepared_frames = match mode {
        ClassicAudioPreparationMode::UnpreparedLinear => 0,
        ClassicAudioPreparationMode::PreparedLinear => u128::from(output_frames),
        ClassicAudioPreparationMode::PreparedSoundtouch => {
            u128::from(actual_source_frames)
                + u128::from(resampled_frames)
                + u128::from(output_frames)
        }
    };
    let prepared_channel_frames = prepared_frames * u128::from(num_channels);
    let total_channel_frames = count(decoded_channel_frames + prepared_channel_frames)?;
    let decoded_bytes = count(decoded_channel_frames * 4)?;
    let prepared_bytes = count(prepared_channel_frames * 4)?;
    let total_bytes = count(u128::from(total_channel_frames) * 4)?;
    if (mode == ClassicAudioPreparationMode::PreparedSoundtouch
        && (actual_source_frames > budget.max_source_frames
            || resampled_frames > budget.max_source_frames))
        || (mode != ClassicAudioPreparationMode::UnpreparedLinear
            && output_frames > budget.max_output_frames)
        || total_channel_frames > budget.max_total_channel_frames
    {
        return Err(invalid(
            "Classic audio preparation exceeds prefix/frame/channel budget",
        ));
    }
    Ok(ClassicAudioPreparationFacts {
        mode,
        source_slice_start_sample,
        requested_source_frames,
        actual_source_frames,
        resampled_frames,
        output_frames,
        num_channels,
        decoded_sample_rate: input.decoded_sample_rate,
        target_sample_rate: input.target_sample_rate,
        tempo,
        prefix_frames,
        total_channel_frames,
        decoded_bytes,
        prepared_bytes,
        total_bytes,
    })
}

mod decimal {
    use serde::{Deserialize, Deserializer, Serializer, de::Error};
    pub fn serialize<S: Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&value.to_string())
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(D::Error::custom)
    }
}

mod rational_decimal {
    use super::RationalRate;
    use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};
    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Rate {
        numerator: String,
        denominator: String,
    }
    pub fn serialize<S: Serializer>(
        value: &RationalRate,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        Rate {
            numerator: value.numerator.to_string(),
            denominator: value.denominator.to_string(),
        }
        .serialize(serializer)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<RationalRate, D::Error> {
        let rate = Rate::deserialize(deserializer)?;
        Ok(RationalRate {
            numerator: rate.numerator.parse().map_err(D::Error::custom)?,
            denominator: rate.denominator.parse().map_err(D::Error::custom)?,
        })
    }
}

#[cfg(test)]
mod tests;
