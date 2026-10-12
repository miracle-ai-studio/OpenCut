//! Validate standalone sampling inputs without assuming aggregate validation.
use super::*;

fn target(value: &ChannelTargetV4) -> Result<(), EditorError> {
    let ids = match value {
        ChannelTargetV4::Element { element_id } => vec![element_id],
        ChannelTargetV4::Effect {
            element_id,
            effect_id,
        } => vec![element_id, effect_id],
        ChannelTargetV4::Mask {
            element_id,
            mask_id,
        } => vec![element_id, mask_id],
        ChannelTargetV4::Track { track_id } => vec![track_id],
    };
    for id in ids {
        if id.trim().is_empty() || id.len() > 128 {
            return Err(invalid("Invalid animation target identity"));
        }
    }
    Ok(())
}

fn key_times<'a>(
    id: &str,
    target_value: &ChannelTargetV4,
    keys: impl Iterator<Item = (&'a str, &'a Time)>,
) -> Result<Vec<RationalTime>, EditorError> {
    target(target_value)?;
    let mut ids = HashSet::new();
    unique(&mut ids, id)?;
    let mut times: Vec<RationalTime> = vec![];
    for (id, value) in keys {
        unique(&mut ids, id)?;
        let at = RationalTime::try_from(value)?;
        if at.ticks() < 0 {
            return Err(invalid("Animation key time must be nonnegative"));
        }
        if times
            .last()
            .is_some_and(|previous| previous.cmp_exact(at) != Ordering::Less)
        {
            return Err(invalid(
                "Animation key times must be strictly increasing and distinct",
            ));
        }
        times.push(at);
    }
    if times.is_empty() {
        return Err(invalid("Animation channel requires keys"));
    }
    Ok(times)
}

pub(super) fn scalar(
    id: &str,
    target: &ChannelTargetV4,
    binding: &ScalarBindingV4,
    keys: &[ScalarKeyV4],
) -> Result<Vec<RationalTime>, EditorError> {
    let times = key_times(
        id,
        target,
        keys.iter().map(|key| (key.id.as_str(), &key.time)),
    )?;
    let mut ids = HashSet::from([id.to_owned()]);
    ids.extend(keys.iter().map(|key| key.id.clone()));
    for (index, key) in keys.iter().enumerate() {
        if let Some(segment_id) = &key.segment_id {
            unique(&mut ids, segment_id)?;
        }
        validate_stored_scalar(binding, key.value)?;
        for (handle, left) in [(&key.left_handle, true), (&key.right_handle, false)] {
            let Some(handle) = handle else { continue };
            let offset = RationalTime::try_from(&handle.dt)?;
            finite(handle.dv, "Tangent value offset")?;
            finite(key.value + handle.dv, "Tangent control value")?;
            if (left && offset.ticks() > 0) || (!left && offset.ticks() < 0) {
                return Err(invalid("Tangent time offset has the wrong direction"));
            }
            let neighbor = if left {
                index.checked_sub(1)
            } else {
                (index + 1 < times.len()).then_some(index + 1)
            };
            if let Some(neighbor) = neighbor {
                let (span, base) = if left {
                    difference(times[index], times[neighbor])
                } else {
                    difference(times[neighbor], times[index])
                };
                // Compare exact offsets, without requiring a representable LCM.
                let magnitude = i128::from(offset.ticks()).abs();
                if magnitude * base > span * i128::from(offset.timebase()) {
                    return Err(invalid("Tangent time control exceeds its adjacent segment"));
                }
            }
        }
        if let ScalarSegmentV4::LegacyNormalizedBezier { curve } = &key.segment_to_next {
            for value in [curve.x1, curve.y1, curve.x2, curve.y2] {
                bound(value, 0.0, 1.0, "Legacy normalized Bezier control")?;
            }
        }
        if let Some(next) = keys.get(index + 1) {
            finite(next.value - key.value, "Channel segment delta")?;
        }
    }
    Ok(times)
}

pub(super) fn discrete(
    id: &str,
    target: &ChannelTargetV4,
    keys: &[DiscreteKeyV4],
) -> Result<Vec<RationalTime>, EditorError> {
    key_times(
        id,
        target,
        keys.iter().map(|key| (key.id.as_str(), &key.time)),
    )
}
