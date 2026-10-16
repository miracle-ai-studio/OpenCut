use super::*;
use std::cmp::Ordering;
fn unique_keys(keys: &[KeyframeRefV1]) -> Result<(), EditorError> {
    if keys.is_empty() || keys.len() > 4096 {
        return Err(invalid("Select 1..4096 keys"));
    }
    let mut ids = BTreeSet::new();
    for k in keys {
        bounded(&k.channel_id)?;
        bounded(&k.key_id)?;
        if !ids.insert((&k.channel_id, &k.key_id)) {
            return Err(invalid("A selected key occurs twice"));
        }
    }
    Ok(())
}
fn sort_scalar(keys: &mut [ScalarKeyV4]) -> Result<(), EditorError> {
    for k in keys.iter() {
        R::try_from(&k.time)?;
    }
    keys.sort_by(|a, b| {
        R::try_from(&a.time)
            .unwrap()
            .cmp_exact(R::try_from(&b.time).unwrap())
    });
    Ok(())
}
fn sort_discrete(keys: &mut [DiscreteKeyV4]) -> Result<(), EditorError> {
    for k in keys.iter() {
        R::try_from(&k.time)?;
    }
    keys.sort_by(|a, b| {
        R::try_from(&a.time)
            .unwrap()
            .cmp_exact(R::try_from(&b.time).unwrap())
    });
    Ok(())
}
fn scaled(span: R, fraction: f64) -> Result<Time, EditorError> {
    // Classic roundMediaTime uses 120000 ticks/s. Preserve that explicit
    // authoring quantization; subsequent evaluation remains exact rational.
    let ticks = ((span.ticks() as f64 / span.timebase() as f64) * fraction * 120000.0).round();
    if !ticks.is_finite() || ticks < i64::MIN as f64 || ticks >= i64::MAX as f64 {
        return Err(invalid("Curve handle time overflow"));
    }
    Ok(Time {
        ticks: (ticks as i64).to_string(),
        timebase: 120000,
    })
}
fn curve(
    keys: &mut [ScalarKeyV4],
    left_id: &str,
    right_id: &str,
    c: &CurvePresetV1,
    reference: Option<f64>,
) -> Result<(), EditorError> {
    if ![c.x1, c.x2, c.y1, c.y2].iter().all(|n| n.is_finite())
        || !(0.0..=1.0).contains(&c.x1)
        || !(0.0..=1.0).contains(&c.x2)
        || !(-0.5..=1.5).contains(&c.y1)
        || !(-0.5..=1.5).contains(&c.y2)
    {
        return Err(invalid("Classic curves require X 0..1 and Y -0.5..1.5"));
    }
    let at = keys
        .iter()
        .position(|k| k.id == left_id)
        .ok_or_else(missing)?;
    if keys.get(at + 1).is_none_or(|k| k.id != right_id) {
        return Err(invalid(
            "Curve endpoints must be adjacent in one scalar channel",
        ));
    }
    let (left, right) = keys.split_at_mut(at + 1);
    let left = &mut left[at];
    let right = &mut right[0];
    let span = R::try_from(&right.time)?.checked_sub(R::try_from(&left.time)?)?;
    if span.ticks() <= 0 {
        return Err(invalid("Curve segment duration must be positive"));
    }
    let delta = right.value - left.value;
    let effective = if delta.abs() > 1e-6 {
        delta
    } else {
        reference
            .filter(|v| v.is_finite() && v.abs() > 1e-6)
            .ok_or_else(|| {
                invalid("Flat curve needs an explicit finite nonzero reference value span")
            })?
    };
    let out_dv = effective * c.y1;
    let in_dv = effective * (c.y2 - 1.0);
    if !out_dv.is_finite() || !in_dv.is_finite() {
        return Err(invalid("Curve value span overflow"));
    }
    let out = scaled(span, c.x1)?;
    let incoming = scaled(span, c.x2 - 1.0)?;
    if R::try_from(&out)?.cmp_exact(span).is_gt()
        || R::try_from(&incoming)?.checked_add(span)?.ticks() < 0
    {
        return Err(invalid(
            "Curve handle quantization exceeds this short segment",
        ));
    }
    left.right_handle = Some(CurveHandleV4 {
        dt: out,
        dv: out_dv,
    });
    right.left_handle = Some(CurveHandleV4 {
        dt: incoming,
        dv: in_dv,
    });
    left.segment_to_next = ScalarSegmentV4::Bezier;
    left.tangent_mode = TangentModeV4::Broken;
    right.tangent_mode = TangentModeV4::Broken;
    Ok(())
}
fn ensure_new_scalar(keys: &[ScalarKeyV4], key: &ScalarKeyV4) -> Result<(), EditorError> {
    bounded(&key.id)?;
    if keys.iter().any(|old| {
        old.id != key.id
            && R::try_from(&old.time)
                .ok()
                .zip(R::try_from(&key.time).ok())
                .is_some_and(|(a, b)| a.cmp_exact(b) == Ordering::Equal)
    }) {
        return Err(invalid(
            "Pasting onto an occupied key time rejects the whole batch",
        ));
    }
    Ok(())
}
fn ensure_new_discrete(keys: &[DiscreteKeyV4], key: &DiscreteKeyV4) -> Result<(), EditorError> {
    bounded(&key.id)?;
    if keys.iter().any(|old| {
        old.id != key.id
            && R::try_from(&old.time)
                .ok()
                .zip(R::try_from(&key.time).ok())
                .is_some_and(|(a, b)| a.cmp_exact(b) == Ordering::Equal)
    }) {
        return Err(invalid(
            "Pasting onto an occupied key time rejects the whole batch",
        ));
    }
    Ok(())
}
pub(super) fn apply(sequence: &mut SequenceV4, edit: &KeyframeEditV1) -> Result<(), EditorError> {
    match edit {
        KeyframeEditV1::ChannelPut { owner, channel } => {
            bounded(cid(channel))?;
            let channels = match owner {
                KeyframeOwnerV1::Track { track_id } => {
                    let track = sequence
                        .tracks
                        .iter_mut()
                        .find(|t| &t.id == track_id)
                        .ok_or_else(missing)?;
                    if track.locked {
                        return Err(EditorError::new(
                            "TRACK_LOCKED",
                            "Unlock the keyframe track",
                        ));
                    }
                    &mut track.channels
                }
                KeyframeOwnerV1::Element { element_id } => {
                    let track = sequence
                        .tracks
                        .iter_mut()
                        .find(|t| t.elements.iter().any(|e| &e.id == element_id))
                        .ok_or_else(missing)?;
                    if track.locked {
                        return Err(EditorError::new(
                            "TRACK_LOCKED",
                            "Unlock the keyframe track",
                        ));
                    }
                    &mut track
                        .elements
                        .iter_mut()
                        .find(|e| &e.id == element_id)
                        .unwrap()
                        .channels
                }
            };
            if let Some(old) = channels.iter_mut().find(|c| cid(c) == cid(channel)) {
                *old = channel.clone();
            } else {
                channels.push(channel.clone());
            }
        }
        KeyframeEditV1::Move {
            keys: selected,
            delta,
        } => {
            unique_keys(selected)?;
            let delta = R::try_from(delta)?;
            for selected in selected {
                match channels_mut(sequence, &selected.channel_id)? {
                    AnimationChannelV4::Scalar { keys, .. } => {
                        let k = keys
                            .iter_mut()
                            .find(|k| k.id == selected.key_id)
                            .ok_or_else(missing)?;
                        k.time = R::try_from(&k.time)?.checked_add(delta)?.into();
                    }
                    AnimationChannelV4::Discrete { keys, .. } => {
                        let k = keys
                            .iter_mut()
                            .find(|k| k.id == selected.key_id)
                            .ok_or_else(missing)?;
                        k.time = R::try_from(&k.time)?.checked_add(delta)?.into();
                    }
                }
            }
            let channels: BTreeSet<_> = selected.iter().map(|r| r.channel_id.as_str()).collect();
            for id in channels {
                match channels_mut(sequence, id)? {
                    AnimationChannelV4::Scalar { keys, .. } => sort_scalar(keys)?,
                    AnimationChannelV4::Discrete { keys, .. } => sort_discrete(keys)?,
                }
            }
        }
        KeyframeEditV1::Delete { keys: selected } => {
            unique_keys(selected)?;
            for r in selected {
                match channels_mut(sequence, &r.channel_id)? {
                    AnimationChannelV4::Scalar { keys, .. } => {
                        let at = keys
                            .iter()
                            .position(|k| k.id == r.key_id)
                            .ok_or_else(missing)?;
                        keys.remove(at);
                    }
                    AnimationChannelV4::Discrete { keys, .. } => {
                        let at = keys
                            .iter()
                            .position(|k| k.id == r.key_id)
                            .ok_or_else(missing)?;
                        keys.remove(at);
                    }
                }
            }
            // An empty channel carries no animation. Remove it in the same owner
            // operation, matching Classic normalization rather than invalid V4.
            for t in &mut sequence.tracks {
                t.channels.retain(nonempty);
                for e in &mut t.elements {
                    e.channels.retain(nonempty);
                }
            }
        }
        KeyframeEditV1::PutScalar { channel_id, key } => {
            let AnimationChannelV4::Scalar { keys, .. } = channels_mut(sequence, channel_id)?
            else {
                return Err(invalid("Scalar key cannot enter a discrete channel"));
            };
            ensure_new_scalar(keys, key)?;
            if let Some(old) = keys.iter_mut().find(|k| k.id == key.id) {
                *old = key.clone();
            } else {
                keys.push(key.clone());
            }
            sort_scalar(keys)?;
        }
        KeyframeEditV1::PutDiscrete { channel_id, key } => {
            let AnimationChannelV4::Discrete { keys, .. } = channels_mut(sequence, channel_id)?
            else {
                return Err(invalid("Discrete key cannot enter a scalar channel"));
            };
            ensure_new_discrete(keys, key)?;
            if let Some(old) = keys.iter_mut().find(|k| k.id == key.id) {
                *old = key.clone();
            } else {
                keys.push(key.clone());
            }
            sort_discrete(keys)?;
        }
        KeyframeEditV1::Curve {
            channel_id,
            left_key_id,
            right_key_id,
            curve: c,
            reference_span_value,
        } => {
            let AnimationChannelV4::Scalar { keys, .. } = channels_mut(sequence, channel_id)?
            else {
                return Err(invalid("Discrete segments have no curve"));
            };
            curve(keys, left_key_id, right_key_id, c, *reference_span_value)?;
        }
        KeyframeEditV1::PasteScalar {
            channel_id,
            keys: copied,
            source_anchor,
            destination_anchor,
        } => {
            if copied.is_empty() || copied.len() > 4096 {
                return Err(invalid("Paste needs 1..4096 keys"));
            }
            let offset =
                R::try_from(destination_anchor)?.checked_sub(R::try_from(source_anchor)?)?;
            let AnimationChannelV4::Scalar { keys, .. } = channels_mut(sequence, channel_id)?
            else {
                return Err(invalid("Scalar clipboard cannot enter a discrete channel"));
            };
            let mut ids = BTreeSet::new();
            for key in copied {
                if !ids.insert(key.id.clone()) || keys.iter().any(|k| k.id == key.id) {
                    return Err(invalid("Paste requires fresh unique key IDs"));
                }
                let mut key = key.clone();
                key.time = R::try_from(&key.time)?.checked_add(offset)?.into();
                ensure_new_scalar(keys, &key)?;
                keys.push(key);
            }
            sort_scalar(keys)?;
        }
        KeyframeEditV1::PasteDiscrete {
            channel_id,
            keys: copied,
            source_anchor,
            destination_anchor,
        } => {
            if copied.is_empty() || copied.len() > 4096 {
                return Err(invalid("Paste needs 1..4096 keys"));
            }
            let offset =
                R::try_from(destination_anchor)?.checked_sub(R::try_from(source_anchor)?)?;
            let AnimationChannelV4::Discrete { keys, .. } = channels_mut(sequence, channel_id)?
            else {
                return Err(invalid("Discrete clipboard cannot enter a scalar channel"));
            };
            let mut ids = BTreeSet::new();
            for key in copied {
                if !ids.insert(key.id.clone()) || keys.iter().any(|k| k.id == key.id) {
                    return Err(invalid("Paste requires fresh unique key IDs"));
                }
                let mut key = key.clone();
                key.time = R::try_from(&key.time)?.checked_add(offset)?.into();
                ensure_new_discrete(keys, &key)?;
                keys.push(key);
            }
            sort_discrete(keys)?;
        }
    }
    Ok(())
}
fn nonempty(c: &AnimationChannelV4) -> bool {
    match c {
        AnimationChannelV4::Scalar { keys, .. } => !keys.is_empty(),
        AnimationChannelV4::Discrete { keys, .. } => !keys.is_empty(),
    }
}
