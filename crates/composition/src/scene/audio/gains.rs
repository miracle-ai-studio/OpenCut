use super::*;
use opencut_editor_core::scene::ValidatedAnimationChannel;
pub(super) fn channels(channels: &[AnimationChannel]) -> Vec<AnimationChannel> {
    channels
        .iter()
        .filter(|channel| {
            matches!(
                channel,
                AnimationChannel::Scalar {
                    binding: ScalarBinding::Property {
                        parameter: ScalarParameter::GainDecibels
                            | ScalarParameter::GainLinearLegacy
                    },
                    ..
                }
            )
        })
        .cloned()
        .collect()
}
pub(super) fn physical(
    base: Gain,
    channels: &[AnimationChannel],
    at: Option<&Time>,
) -> Result<Gain, CompositionError> {
    let validated = validate(channels)?;
    sampled_physical(base, &validated, at)
}
fn validate(
    channels: &[AnimationChannel],
) -> Result<Vec<ValidatedAnimationChannel<'_>>, CompositionError> {
    channels
        .iter()
        .map(|channel| ValidatedAnimationChannel::new(channel).map_err(error))
        .collect()
}
fn sampled_physical(
    base: Gain,
    channels: &[ValidatedAnimationChannel<'_>],
    at: Option<&Time>,
) -> Result<Gain, CompositionError> {
    let mut gain = base;
    if let Some(at) = at {
        for channel in channels {
            let SampledChannel::Scalar {
                binding: ScalarBinding::Property { parameter },
                physical,
                ..
            } = channel.sample(at).map_err(error)?
            else {
                return Err(invalid("Audio gain channel mismatch"));
            };
            gain = match (parameter, gain) {
                (ScalarParameter::GainDecibels, Gain::Decibels { .. }) => {
                    Gain::Decibels { value: physical }
                }
                (ScalarParameter::GainLinearLegacy, Gain::LinearLegacy { .. }) => {
                    Gain::LinearLegacy { value: physical }
                }
                _ => return Err(invalid("Audio gain unit mismatch")),
            };
        }
    }
    Ok(gain)
}
fn amplitude(
    base: Gain,
    channels: &[ValidatedAnimationChannel<'_>],
    at: Option<&Time>,
) -> Result<f64, CompositionError> {
    Ok(animation::gain(sampled_physical(base, channels, at)?)?.amplitude)
}

/// A validated physical curve shared by the exact export and legacy preview
/// clocks. The proof borrows immutable authored keys for one compiled block.
pub(super) struct PreparedGainCurve<'a> {
    base: Gain,
    channels: Vec<ValidatedAnimationChannel<'a>>,
}
impl<'a> PreparedGainCurve<'a> {
    pub(super) fn new(
        base: Gain,
        channels: &'a [AnimationChannel],
    ) -> Result<Self, CompositionError> {
        Ok(Self {
            base,
            channels: validate(channels)?,
        })
    }
    pub(super) fn sample(&self, sample: u64, rate: u32) -> Result<f64, CompositionError> {
        let at = clocks::sample_time(sample, rate, true)?;
        amplitude(self.base, &self.channels, Some(&at))
    }
}
pub(super) fn compile(
    voice: &PreparedVoice,
    identity: &AudioIdentity,
    start: u64,
    local: u64,
    frames: u32,
) -> Result<SampleGain, CompositionError> {
    if voice.element_channels.is_empty() && voice.track_channels.is_empty() {
        return Ok(SampleGain::Constant {
            amplitude: finite(
                amplitude(voice.element_gain, &[], None)? * amplitude(voice.track_gain, &[], None)?,
            )?,
        });
    }
    let classic = identity.profile == AudioExecutionProfile::ClassicV31;
    let element_channels = validate(&voice.element_channels)?;
    let track_channels = validate(&voice.track_channels)?;
    let mut amplitudes = Vec::with_capacity(frames as usize);
    for offset in 0..u64::from(frames) {
        let local_time = if voice.element_channels.is_empty() {
            None
        } else {
            Some(clocks::sample_time(
                local
                    .checked_add(offset)
                    .ok_or_else(|| invalid("Local sample overflow"))?,
                identity.format.sample_rate,
                classic,
            )?)
        };
        let track_time = if voice.track_channels.is_empty() {
            None
        } else {
            Some(clocks::sample_time(
                start
                    .checked_add(offset)
                    .ok_or_else(|| invalid("Sequence sample overflow"))?,
                identity.format.sample_rate,
                classic,
            )?)
        };
        amplitudes.push(finite(
            amplitude(voice.element_gain, &element_channels, local_time.as_ref())?
                * amplitude(voice.track_gain, &track_channels, track_time.as_ref())?,
        )?);
    }
    Ok(SampleGain::Dense { amplitudes })
}
