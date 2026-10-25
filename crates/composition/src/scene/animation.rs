//! Every applicable V4 channel is evaluated and written to its actual typed
//! target. Transient copies are consumed into IR; channels never enter IR.
use super::*;
mod projection;
mod properties;
use projection::project_static;
use properties::{apply, mismatch, set_gain};

pub(super) fn gain(value: Gain) -> Result<CompiledGain, CompositionError> {
    let physical = match value {
        Gain::LinearLegacy { value } => Gain::LinearLegacy {
            value: property(ScalarParameter::GainLinearLegacy, value)?,
        },
        Gain::Decibels { value } => Gain::Decibels {
            value: property(ScalarParameter::GainDecibels, value)?,
        },
    };
    Ok(CompiledGain {
        physical,
        amplitude: gain_amplitude(&physical).map_err(error)?,
    })
}
pub(super) fn track(
    track: &SceneTrack,
    at: &Time,
    scope: CompositionScope,
    audio_exclusion: Option<Exclusion>,
    entries: &mut Vec<SourceCoverage>,
) -> Result<(f64, Option<CompiledGain>), CompositionError> {
    let mut opacity = property(ScalarParameter::Opacity, track.opacity)?;
    let audio = scope == CompositionScope::AudioVisual
        && matches!(track.kind, SceneTrackKind::Video | SceneTrackKind::Audio)
        && audio_exclusion.is_none();
    let mut sampled_gain = track.gain;
    for channel in &track.channels {
        let AnimationChannel::Scalar {
            binding: ScalarBinding::Property { parameter },
            ..
        } = channel
        else {
            return Err(mismatch());
        };
        if matches!(
            parameter,
            ScalarParameter::GainLinearLegacy | ScalarParameter::GainDecibels
        ) && !audio
        {
            coverage::channel(
                entries,
                channel,
                None,
                audio_exclusion.unwrap_or(Exclusion::VisualScope),
            );
            continue;
        }
        let SampledChannel::Scalar { physical, .. } =
            sample_animation_channel_scene(channel, at).map_err(error)?
        else {
            return Err(mismatch());
        };
        match parameter {
            ScalarParameter::Opacity => opacity = physical,
            ScalarParameter::GainLinearLegacy | ScalarParameter::GainDecibels => {
                set_gain(&mut sampled_gain, *parameter, physical)?
            }
            _ => return Err(mismatch()),
        }
        coverage::channel(entries, channel, Some(&track.id), Exclusion::VisualScope);
    }
    Ok((
        opacity,
        if audio {
            Some(gain(sampled_gain)?)
        } else {
            None
        },
    ))
}

pub(super) fn element(
    value: &TimelineElement,
    at: &Time,
    scope: CompositionScope,
    audio_exclusion: Option<Exclusion>,
    entries: &mut Vec<SourceCoverage>,
) -> Result<TimelineElement, CompositionError> {
    let mut output = value.clone();
    project_static(
        &mut output,
        if audio_exclusion.is_some() {
            CompositionScope::Visual
        } else {
            scope
        },
    )?;
    for channel in &value.channels {
        let target = match channel {
            AnimationChannel::Scalar { target, .. } | AnimationChannel::Discrete { target, .. } => {
                target
            }
        };
        let excluded = match target {
            ChannelTarget::Effect { effect_id, .. } => {
                let enabled = match &value.content {
                    ElementContent::EffectLayer { effect } if effect.id == *effect_id => {
                        effect.enabled
                    }
                    _ => value
                        .visual
                        .as_ref()
                        .and_then(|v| v.effects.iter().find(|e| e.id == *effect_id))
                        .is_some_and(|e| e.enabled),
                };
                (!enabled).then_some(Exclusion::DisabledEffect)
            }
            ChannelTarget::Mask { mask_id, .. } => (!value
                .visual
                .as_ref()
                .and_then(|v| v.masks.iter().find(|m| m.id == *mask_id))
                .is_some_and(|m| m.enabled))
            .then_some(Exclusion::DisabledMask),
            _ => None,
        }
        .or_else(|| {
            (audio_exclusion.is_some()
                && matches!(
                    channel,
                    AnimationChannel::Scalar {
                        binding: ScalarBinding::Property {
                            parameter: ScalarParameter::GainLinearLegacy
                                | ScalarParameter::GainDecibels
                        },
                        ..
                    }
                ))
            .then(|| audio_exclusion.unwrap())
        });
        if let Some(reason) = excluded {
            coverage::channel(entries, channel, None, reason);
            continue;
        }
        apply(
            &mut output,
            target,
            sample_animation_channel_scene(channel, at).map_err(error)?,
        )?;
        coverage::channel(entries, channel, Some(&value.id), Exclusion::OutsideFrame);
    }
    output.channels.clear();
    Ok(output)
}
