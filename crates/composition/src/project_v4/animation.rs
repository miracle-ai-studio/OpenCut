//! Every applicable V4 channel is evaluated and written to its actual typed
//! target. Transient copies are consumed into IR; channels never enter IR.
use super::*;
mod projection;
mod properties;
use projection::project_static;
use properties::{apply, mismatch, set_gain};

pub(super) fn gain(value: GainV4) -> Result<CompiledGainV4, CompositionError> {
    let physical = match value {
        GainV4::LinearLegacy { value } => GainV4::LinearLegacy {
            value: property(ScalarParameterV4::GainLinearLegacy, value)?,
        },
        GainV4::Decibels { value } => GainV4::Decibels {
            value: property(ScalarParameterV4::GainDecibels, value)?,
        },
    };
    Ok(CompiledGainV4 {
        physical,
        amplitude: gain_amplitude(&physical).map_err(error)?,
    })
}
pub(super) fn track(
    track: &TrackV4,
    at: &Time,
    scope: CompositionScope,
    audio_exclusion: Option<ExclusionV4>,
    entries: &mut Vec<SourceCoverageV4>,
) -> Result<(f64, Option<CompiledGainV4>), CompositionError> {
    let mut opacity = property(ScalarParameterV4::Opacity, track.opacity)?;
    let audio = scope == CompositionScope::AudioVisual
        && matches!(track.kind, TrackKindV4::Video | TrackKindV4::Audio)
        && audio_exclusion.is_none();
    let mut sampled_gain = track.gain;
    for channel in &track.channels {
        let AnimationChannelV4::Scalar {
            binding: ScalarBindingV4::Property { parameter },
            ..
        } = channel
        else {
            return Err(mismatch());
        };
        if matches!(
            parameter,
            ScalarParameterV4::GainLinearLegacy | ScalarParameterV4::GainDecibels
        ) && !audio
        {
            coverage::channel(
                entries,
                channel,
                None,
                audio_exclusion.unwrap_or(ExclusionV4::VisualScope),
            );
            continue;
        }
        let SampledChannelV4::Scalar { physical, .. } =
            sample_animation_channel_v4(channel, at).map_err(error)?
        else {
            return Err(mismatch());
        };
        match parameter {
            ScalarParameterV4::Opacity => opacity = physical,
            ScalarParameterV4::GainLinearLegacy | ScalarParameterV4::GainDecibels => {
                set_gain(&mut sampled_gain, *parameter, physical)?
            }
            _ => return Err(mismatch()),
        }
        coverage::channel(entries, channel, Some(&track.id), ExclusionV4::VisualScope);
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
    value: &TimelineElementV4,
    at: &Time,
    scope: CompositionScope,
    audio_exclusion: Option<ExclusionV4>,
    entries: &mut Vec<SourceCoverageV4>,
) -> Result<TimelineElementV4, CompositionError> {
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
            AnimationChannelV4::Scalar { target, .. }
            | AnimationChannelV4::Discrete { target, .. } => target,
        };
        let excluded = match target {
            ChannelTargetV4::Effect { effect_id, .. } => {
                let enabled = match &value.content {
                    ElementContentV4::EffectLayer { effect } if effect.id == *effect_id => {
                        effect.enabled
                    }
                    _ => value
                        .visual
                        .as_ref()
                        .and_then(|v| v.effects.iter().find(|e| e.id == *effect_id))
                        .is_some_and(|e| e.enabled),
                };
                (!enabled).then_some(ExclusionV4::DisabledEffect)
            }
            ChannelTargetV4::Mask { mask_id, .. } => (!value
                .visual
                .as_ref()
                .and_then(|v| v.masks.iter().find(|m| m.id == *mask_id))
                .is_some_and(|m| m.enabled))
            .then_some(ExclusionV4::DisabledMask),
            _ => None,
        }
        .or_else(|| {
            (audio_exclusion.is_some()
                && matches!(
                    channel,
                    AnimationChannelV4::Scalar {
                        binding: ScalarBindingV4::Property {
                            parameter: ScalarParameterV4::GainLinearLegacy
                                | ScalarParameterV4::GainDecibels
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
            sample_animation_channel_v4(channel, at).map_err(error)?,
        )?;
        coverage::channel(entries, channel, Some(&value.id), ExclusionV4::OutsideFrame);
    }
    output.channels.clear();
    Ok(output)
}
