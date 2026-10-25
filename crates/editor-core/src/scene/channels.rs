//! Typed animation ownership, units and local-clock invariants.
use super::parameters::{
    ParameterTarget as Target, discrete_applies, scalar_applies, validate_stored_discrete,
    validate_stored_scalar,
};
use super::*;

fn element_target<'a>(
    owner: &'a TimelineElement,
    target: &ChannelTarget,
) -> Result<Target<'a>, EditorError> {
    match target {
        ChannelTarget::Element { element_id } if element_id == &owner.id => {
            Ok(Target::Element(owner))
        }
        ChannelTarget::Effect {
            element_id,
            effect_id,
        } if element_id == &owner.id => {
            let node = owner
                .visual
                .as_ref()
                .and_then(|v| v.effects.iter().find(|e| &e.id == effect_id))
                .or_else(|| match &owner.content {
                    ElementContent::EffectLayer { effect } if &effect.id == effect_id => {
                        Some(effect)
                    }
                    _ => None,
                });
            node.map(Target::Effect)
                .ok_or_else(|| invalid("Channel effect must belong to its owning element"))
        }
        ChannelTarget::Mask {
            element_id,
            mask_id,
        } if element_id == &owner.id => owner
            .visual
            .as_ref()
            .and_then(|v| v.masks.iter().find(|m| &m.id == mask_id))
            .map(Target::Mask)
            .ok_or_else(|| invalid("Channel mask must belong to its owning element")),
        _ => Err(invalid(
            "Element channels cannot target another element or a track",
        )),
    }
}
fn key_times<'a>(
    keys: impl Iterator<Item = (&'a str, &'a Time)>,
    end: RationalTime,
    ids: &mut HashSet<String>,
) -> Result<Vec<RationalTime>, EditorError> {
    let mut times: Vec<RationalTime> = Vec::new();
    for (id, value) in keys {
        unique(ids, id)?;
        let value = time(value)?;
        if value.ticks() < 0
            || value.cmp_exact(end) == Ordering::Greater
            || times
                .last()
                .is_some_and(|previous| previous.cmp_exact(value) != Ordering::Less)
        {
            return Err(invalid(
                "Channel keys must be strictly ordered within their owning clock",
            ));
        }
        times.push(value);
    }
    if times.is_empty() {
        return Err(invalid("Animation channels require keys"));
    }
    Ok(times)
}
fn handles(
    keys: &[ScalarKey],
    times: &[RationalTime],
    ids: &mut HashSet<String>,
) -> Result<(), EditorError> {
    for (index, key) in keys.iter().enumerate() {
        if let Some(id) = &key.segment_id {
            unique(ids, id)?;
        }
        for (handle, left) in [(&key.left_handle, true), (&key.right_handle, false)] {
            let Some(handle) = handle else {
                continue;
            };
            let offset = time(&handle.dt)?;
            finite(handle.dv, "Tangent value offset")?;
            finite(key.value + handle.dv, "Tangent control value")?;
            if (left && offset.ticks() > 0) || (!left && offset.ticks() < 0) {
                return Err(invalid("Tangent time offset has the wrong direction"));
            }
            let point = checked_sum(times[index], offset)?;
            let neighbor = if left {
                index.checked_sub(1)
            } else {
                (index + 1 < times.len()).then_some(index + 1)
            };
            if let Some(neighbor) = neighbor
                && ((left && point.cmp_exact(times[neighbor]) == Ordering::Less)
                    || (!left && point.cmp_exact(times[neighbor]) == Ordering::Greater))
            {
                return Err(invalid("Tangent time control exceeds its adjacent segment"));
            }
        }
        if let ScalarSegment::LegacyNormalizedBezier { curve } = &key.segment_to_next {
            for value in [curve.x1, curve.x2, curve.y1, curve.y2] {
                bound(value, 0.0, 1.0, "Legacy normalized Bezier control")?;
            }
        }
        // TangentMode is an editing hint. Stored actual handles win, including
        // Flat/Aligned hints whose historical handles have not been recomputed.
        // Valid overshoot handles are preserved rather than clamped here.
        if let Some(next) = keys.get(index + 1) {
            finite(next.value - key.value, "Channel segment delta")?;
        }
    }
    Ok(())
}
fn validate_channels<'a>(
    channels: &'a [AnimationChannel],
    end: RationalTime,
    resolve: impl Fn(&ChannelTarget) -> Result<Target<'a>, EditorError>,
    ids: &mut HashSet<String>,
) -> Result<(), EditorError> {
    let mut scalar_bindings = Vec::new();
    let mut discrete_bindings = Vec::new();
    for channel in channels {
        match channel {
            AnimationChannel::Scalar {
                id,
                target,
                binding,
                keys,
                extrapolation: _,
            } => {
                unique(ids, id)?;
                if scalar_bindings.contains(&(target, binding)) {
                    return Err(invalid("Duplicate scalar channel binding"));
                }
                scalar_bindings.push((target, binding));
                let target = resolve(target)?;
                let applies = scalar_applies(binding, &target);
                if !applies {
                    return Err(invalid(
                        "Channel parameter unit or type is incompatible with its target",
                    ));
                }
                let times = key_times(keys.iter().map(|k| (k.id.as_str(), &k.time)), end, ids)?;
                for key in keys {
                    validate_stored_scalar(binding, key.value)?;
                }
                handles(keys, &times, ids)?;
                // Both extrapolation modes are explicit contract variants.
                // One-key linear extrapolation retains the single edge value;
                // multi-key linear extrapolation uses the edge secant, not a
                // time/value handle or a normalized easing control.
            }
            AnimationChannel::Discrete {
                id,
                target,
                parameter,
                keys,
            } => {
                unique(ids, id)?;
                if discrete_bindings.contains(&(target, parameter)) {
                    return Err(invalid("Duplicate discrete channel binding"));
                }
                discrete_bindings.push((target, parameter));
                let target = resolve(target)?;
                let applies = discrete_applies(*parameter, &target);
                if !applies {
                    return Err(invalid(
                        "Discrete channel parameter does not apply to target",
                    ));
                }
                key_times(keys.iter().map(|k| (k.id.as_str(), &k.time)), end, ids)?;
                for key in keys {
                    validate_stored_discrete(*parameter, &key.value)?;
                }
                // DiscreteValueV4 contains only typed StrokeAlign: hold behavior
                // is structural and cannot acquire an arbitrary interpolation.
            }
        }
    }
    Ok(())
}
pub(super) fn element_channels(
    owner: &TimelineElement,
    ids: &mut HashSet<String>,
) -> Result<(), EditorError> {
    validate_channels(
        &owner.channels,
        duration(&owner.timeline_range.duration)?,
        |target| element_target(owner, target),
        ids,
    )
}
pub(super) fn track_channels(
    owner: &SceneTrack,
    end: RationalTime,
    ids: &mut HashSet<String>,
) -> Result<(), EditorError> {
    validate_channels(
        &owner.channels,
        end,
        |target| match target {
            ChannelTarget::Track { track_id } if track_id == &owner.id => Ok(Target::Track(owner)),
            _ => Err(invalid("Track channels must reference their owning track")),
        },
        ids,
    )
}
