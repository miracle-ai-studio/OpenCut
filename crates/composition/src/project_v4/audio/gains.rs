use super::*;
pub(super) fn channels(channels: &[AnimationChannelV4]) -> Vec<AnimationChannelV4> {
    channels
        .iter()
        .filter(|channel| {
            matches!(
                channel,
                AnimationChannelV4::Scalar {
                    binding: ScalarBindingV4::Property {
                        parameter: ScalarParameterV4::GainDecibels
                            | ScalarParameterV4::GainLinearLegacy
                    },
                    ..
                }
            )
        })
        .cloned()
        .collect()
}
pub(super) fn physical(
    base: GainV4,
    channels: &[AnimationChannelV4],
    at: Option<&Time>,
) -> Result<GainV4, CompositionError> {
    let mut gain = base;
    if let Some(at) = at {
        for channel in channels {
            let SampledChannelV4::Scalar {
                binding: ScalarBindingV4::Property { parameter },
                physical,
                ..
            } = sample_animation_channel_v4(channel, at).map_err(error)?
            else {
                return Err(invalid("Audio gain channel mismatch"));
            };
            gain = match (parameter, gain) {
                (ScalarParameterV4::GainDecibels, GainV4::Decibels { .. }) => {
                    GainV4::Decibels { value: physical }
                }
                (ScalarParameterV4::GainLinearLegacy, GainV4::LinearLegacy { .. }) => {
                    GainV4::LinearLegacy { value: physical }
                }
                _ => return Err(invalid("Audio gain unit mismatch")),
            };
        }
    }
    Ok(gain)
}
fn amplitude(
    base: GainV4,
    channels: &[AnimationChannelV4],
    at: Option<&Time>,
) -> Result<f64, CompositionError> {
    Ok(animation::gain(physical(base, channels, at)?)?.amplitude)
}
pub(super) fn compile(
    voice: &PreparedVoice,
    identity: &AudioIdentityV4,
    start: u64,
    local: u64,
    frames: u32,
) -> Result<SampleGainV4, CompositionError> {
    if voice.element_channels.is_empty() && voice.track_channels.is_empty() {
        return Ok(SampleGainV4::Constant {
            amplitude: finite(
                amplitude(voice.element_gain, &[], None)? * amplitude(voice.track_gain, &[], None)?,
            )?,
        });
    }
    let classic = identity.profile == AudioExecutionProfileV4::ClassicV31;
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
            amplitude(
                voice.element_gain,
                &voice.element_channels,
                local_time.as_ref(),
            )? * amplitude(voice.track_gain, &voice.track_channels, track_time.as_ref())?,
        )?);
    }
    Ok(SampleGainV4::Dense { amplitudes })
}
