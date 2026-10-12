//! Exact Classic audio-buffer preparation facts. No decoder, PCM, allocator,
//! native pitch library or authored-project mutation is owned by this module.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClassicAudioPreparationModeV4 {
    UnpreparedLinear,
    PreparedLinear,
    PreparedSoundtouch,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClassicAudioPreparationInputV4 {
    /// Source request trimStart, before conversion to native sample indices.
    pub source_start: Time,
    /// Actual authored timeline duration, never ceil(placement frames)/rate.
    pub timeline_duration: Time,
    #[serde(with = "rational_decimal")]
    pub rate: RationalRateV4,
    pub maintain_pitch: bool,
    pub decoded_sample_rate: u32,
    #[serde(with = "decimal")]
    pub decoded_frames: u64,
    pub decoded_channels: u32,
    pub target_sample_rate: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClassicAudioPreparationBudgetV4 {
    /// Ceiling on the nominal source prefix end, not a claim that these samples
    /// exist. A trim beyond decoded length still has one zero-filled source frame.
    #[serde(with = "decimal")]
    pub max_prefix_frames: u64,
    /// Applied to requested, actual and resampled source frame counts.
    #[serde(with = "decimal")]
    pub max_source_frames: u64,
    #[serde(with = "decimal")]
    pub max_output_frames: u64,
    /// Decoded channels plus sliced/resampled/output channels. Native library
    /// FIFO and decoder temporary work memory need separate adapter admission.
    #[serde(with = "decimal")]
    pub max_total_channel_frames: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClassicAudioPreparationFactsV4 {
    pub mode: ClassicAudioPreparationModeV4,
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
    pub tempo: RationalRateV4,
    #[serde(with = "decimal")]
    pub prefix_frames: u64,
    #[serde(with = "decimal")]
    pub total_channel_frames: u64,
    /// Actual decoded buffer channels are counted even when only its first two
    /// participate in the Classic prepared buffer.
    #[serde(with = "decimal")]
    pub decoded_bytes: u64,
    /// Float32 sliced source + resampled source + final prepared output.
    /// These conservative facts apply to all modes; UnpreparedLinear allocates
    /// none of these buffers and must not secretly perform the prepared roundtrip.
    #[serde(with = "decimal")]
    pub prepared_bytes: u64,
    #[serde(with = "decimal")]
    pub total_bytes: u64,
}

/// Classic clampRetimeRate: preserve authored rate separately, evaluate at
/// [1/100, 5]. The reduced rational is the sole effective sample-clock tempo.
pub fn classic_effective_rate_v4(raw: RationalRateV4) -> Result<RationalRateV4, CompositionError> {
    if raw.numerator == 0 || raw.denominator == 0 {
        return Err(invalid("Classic audio rate must be positive"));
    }
    if u64::from(raw.numerator) * 100 < u64::from(raw.denominator) {
        return Ok(RationalRateV4 {
            numerator: 1,
            denominator: 100,
        });
    }
    if u64::from(raw.numerator) > u64::from(raw.denominator) * 5 {
        return Ok(RationalRateV4 {
            numerator: 5,
            denominator: 1,
        });
    }
    let (mut a, mut b) = (raw.numerator, raw.denominator);
    while b != 0 {
        (a, b) = (b, a % b);
    }
    Ok(RationalRateV4 {
        numerator: raw.numerator / a,
        denominator: raw.denominator / a,
    })
}

/// Export prepares a Float32 buffer whenever keepPitch is true, including
/// identity and epsilon-close tempos. Only abs(effective tempo-1)>1e-6 invokes
/// soundtouch. This classification does not prove that processor is available.
pub fn classic_pitch_processing_mode_v4(
    raw: RationalRateV4,
    maintain_pitch: bool,
) -> Result<ClassicAudioPreparationModeV4, CompositionError> {
    let rate = classic_effective_rate_v4(raw)?;
    if !maintain_pitch {
        return Ok(ClassicAudioPreparationModeV4::UnpreparedLinear);
    }
    // Classic selects its DSP branch with IEEE-754 Number subtraction. This
    // intentionally differs from an exact rational epsilon comparison just
    // below one: 999999/1000000 has abs(rate-1) slightly greater than 1e-6.
    // Source positions and buffer counts remain exact rational calculations.
    let effective = f64::from(rate.numerator) / f64::from(rate.denominator);
    Ok(if (effective - 1.0).abs() > 1e-6 {
        ClassicAudioPreparationModeV4::PreparedSoundtouch
    } else {
        ClassicAudioPreparationModeV4::PreparedLinear
    })
}

fn count(value: u128) -> Result<u64, CompositionError> {
    u64::try_from(value).map_err(|_| invalid("Classic audio preparation count overflows u64"))
}
fn scaled_frames(
    duration: RationalTime,
    rate: RationalRateV4,
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

pub fn classic_audio_preparation_facts_v4(
    input: &ClassicAudioPreparationInputV4,
    budget: &ClassicAudioPreparationBudgetV4,
) -> Result<ClassicAudioPreparationFactsV4, CompositionError> {
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
    let tempo = classic_effective_rate_v4(input.rate)?;
    let mode = classic_pitch_processing_mode_v4(tempo, input.maintain_pitch)?;
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
        RationalRateV4 {
            numerator: 1,
            denominator: 1,
        },
        input.target_sample_rate,
    )?;
    let num_channels = input.decoded_channels.min(2);
    let prefix_frames = source_slice_start_sample
        .checked_add(requested_source_frames)
        .ok_or_else(|| invalid("Classic audio source prefix overflow"))?;
    let decoded_channel_frames =
        u128::from(input.decoded_frames) * u128::from(input.decoded_channels);
    let prepared_channel_frames = (u128::from(actual_source_frames)
        + u128::from(resampled_frames)
        + u128::from(output_frames))
        * u128::from(num_channels);
    let total_channel_frames = count(decoded_channel_frames + prepared_channel_frames)?;
    let decoded_bytes = count(decoded_channel_frames * 4)?;
    let prepared_bytes = count(prepared_channel_frames * 4)?;
    let total_bytes = count(u128::from(total_channel_frames) * 4)?;
    if prefix_frames > budget.max_prefix_frames
        || requested_source_frames > budget.max_source_frames
        || actual_source_frames > budget.max_source_frames
        || resampled_frames > budget.max_source_frames
        || output_frames > budget.max_output_frames
        || total_channel_frames > budget.max_total_channel_frames
    {
        return Err(invalid(
            "Classic audio preparation exceeds prefix/frame/channel budget",
        ));
    }
    Ok(ClassicAudioPreparationFactsV4 {
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
    use super::RationalRateV4;
    use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};
    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Rate {
        numerator: String,
        denominator: String,
    }
    pub fn serialize<S: Serializer>(
        value: &RationalRateV4,
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
    ) -> Result<RationalRateV4, D::Error> {
        let rate = Rate::deserialize(deserializer)?;
        Ok(RationalRateV4 {
            numerator: rate.numerator.parse().map_err(D::Error::custom)?,
            denominator: rate.denominator.parse().map_err(D::Error::custom)?,
        })
    }
}

#[cfg(test)]
mod tests;
