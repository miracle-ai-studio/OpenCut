use super::*;
use std::cmp::Ordering;
fn unique_keys(keys: &[KeyframeRef]) -> Result<(), EditorError> {
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
fn sort_scalar(keys: &mut [ScalarKey]) -> Result<(), EditorError> {
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
fn sort_discrete(keys: &mut [DiscreteKey]) -> Result<(), EditorError> {
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
    keys: &mut [ScalarKey],
    left_id: &str,
    right_id: &str,
    c: &CurvePreset,
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
    left.right_handle = Some(CurveHandle {
        dt: out,
        dv: out_dv,
    });
    right.left_handle = Some(CurveHandle {
        dt: incoming,
        dv: in_dv,
    });
    left.segment_to_next = ScalarSegment::Bezier;
    left.tangent_mode = TangentMode::Broken;
    right.tangent_mode = TangentMode::Broken;
    Ok(())
}
fn ensure_new_scalar(keys: &[ScalarKey], key: &ScalarKey) -> Result<(), EditorError> {
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
fn ensure_new_discrete(keys: &[DiscreteKey], key: &DiscreteKey) -> Result<(), EditorError> {
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
pub(super) fn apply(sequence: &mut Scene, edit: &KeyframeEdit) -> Result<(), EditorError> {
    match edit {
        KeyframeEdit::ChannelPut { owner, channel } => {
            bounded(cid(channel))?;
            let channels = match owner {
                KeyframeOwner::Track { track_id } => {
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
                KeyframeOwner::Element { element_id } => {
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
        KeyframeEdit::Move {
            keys: selected,
            delta,
        } => {
            unique_keys(selected)?;
            let delta = R::try_from(delta)?;
            for selected in selected {
                match channels_mut(sequence, &selected.channel_id)? {
                    AnimationChannel::Scalar { keys, .. } => {
                        let k = keys
                            .iter_mut()
                            .find(|k| k.id == selected.key_id)
                            .ok_or_else(missing)?;
                        k.time = R::try_from(&k.time)?.checked_add(delta)?.into();
                    }
                    AnimationChannel::Discrete { keys, .. } => {
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
                    AnimationChannel::Scalar { keys, .. } => sort_scalar(keys)?,
                    AnimationChannel::Discrete { keys, .. } => sort_discrete(keys)?,
                }
            }
        }
        KeyframeEdit::Delete { keys: selected } => {
            unique_keys(selected)?;
            for r in selected {
                match channels_mut(sequence, &r.channel_id)? {
                    AnimationChannel::Scalar { keys, .. } => {
                        let at = keys
                            .iter()
                            .position(|k| k.id == r.key_id)
                            .ok_or_else(missing)?;
                        keys.remove(at);
                    }
                    AnimationChannel::Discrete { keys, .. } => {
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
        KeyframeEdit::PutScalar { channel_id, key } => {
            let AnimationChannel::Scalar { keys, .. } = channels_mut(sequence, channel_id)? else {
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
        KeyframeEdit::InsertScalar {
            channel_id,
            key_id,
            time,
        } => {
            bounded(key_id)?;
            let channel = channels_mut(sequence, channel_id)?;
            let value =
                match opencut_editor_core::scene::sample_animation_channel_scene(channel, time)? {
                    opencut_editor_core::scene::SampledChannel::Scalar { raw, .. } => raw,
                    _ => return Err(invalid("Choose a scalar channel")),
                };
            let AnimationChannel::Scalar { keys, .. } = channel else {
                return Err(invalid("Choose a scalar channel"));
            };
            let at_time = R::try_from(time)?;
            if keys.iter().any(|key| {
                key.id == *key_id || R::try_from(&key.time).unwrap().cmp_exact(at_time).is_eq()
            }) {
                return Err(invalid("A key already exists at this time or identity"));
            }
            let at = keys
                .iter()
                .position(|key| {
                    R::try_from(&key.time)
                        .unwrap()
                        .cmp_exact(R::try_from(time).unwrap())
                        .is_gt()
                })
                .unwrap_or(keys.len());
            let mut key = ScalarKey {
                id: key_id.clone(),
                segment_id: None,
                time: time.clone(),
                value,
                left_handle: None,
                right_handle: None,
                segment_to_next: ScalarSegment::Linear,
                tangent_mode: TangentMode::Broken,
            };
            if at > 0 && at < keys.len() {
                let (lefts, rights) = keys.split_at_mut(at);
                let left = &mut lefts[at - 1];
                let right = &mut rights[0];
                match left.segment_to_next {
                    ScalarSegment::Step => key.segment_to_next = ScalarSegment::Step,
                    ScalarSegment::Linear => {}
                    ScalarSegment::LegacyNormalizedBezier { .. } => {
                        return Err(invalid(
                            "Convert the historical curve before inserting a control point",
                        ));
                    }
                    ScalarSegment::Bezier => split_controls(left, right, &mut key)?,
                }
            }
            keys.insert(at, key);
        }
        KeyframeEdit::PutDiscrete { channel_id, key } => {
            let AnimationChannel::Discrete { keys, .. } = channels_mut(sequence, channel_id)?
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
        KeyframeEdit::Curve {
            channel_id,
            left_key_id,
            right_key_id,
            curve: c,
            reference_span_value,
        } => {
            let AnimationChannel::Scalar { keys, .. } = channels_mut(sequence, channel_id)? else {
                return Err(invalid("Discrete segments have no curve"));
            };
            curve(keys, left_key_id, right_key_id, c, *reference_span_value)?;
        }
        KeyframeEdit::PasteScalar {
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
            let AnimationChannel::Scalar { keys, .. } = channels_mut(sequence, channel_id)? else {
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
        KeyframeEdit::PasteDiscrete {
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
            let AnimationChannel::Discrete { keys, .. } = channels_mut(sequence, channel_id)?
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
fn nonempty(c: &AnimationChannel) -> bool {
    match c {
        AnimationChannel::Scalar { keys, .. } => !keys.is_empty(),
        AnimationChannel::Discrete { keys, .. } => !keys.is_empty(),
    }
}

/// De Casteljau subdivision retains the authored time/value curve. Time
/// controls use the existing 120000-tick authoring quantization.
fn split_controls(
    left: &mut ScalarKey,
    right: &mut ScalarKey,
    key: &mut ScalarKey,
) -> Result<(), EditorError> {
    let start = R::try_from(&left.time)?;
    let end = R::try_from(&right.time)?;
    let span = end.checked_sub(start)?;
    let local = R::try_from(&key.time)?.checked_sub(start)?;
    let secs = |v: R| v.ticks() as f64 / v.timebase() as f64;
    let duration = secs(span);
    let target = secs(local);
    let dv = right.value - left.value;
    let points = [
        (0.0, left.value),
        (
            left.right_handle
                .as_ref()
                .map(|h| R::try_from(&h.dt).map(secs))
                .transpose()?
                .unwrap_or(duration / 3.0),
            left.value + left.right_handle.as_ref().map(|h| h.dv).unwrap_or(dv / 3.0),
        ),
        (
            duration
                + right
                    .left_handle
                    .as_ref()
                    .map(|h| R::try_from(&h.dt).map(secs))
                    .transpose()?
                    .unwrap_or(-duration / 3.0),
            right.value
                + right
                    .left_handle
                    .as_ref()
                    .map(|h| h.dv)
                    .unwrap_or(-dv / 3.0),
        ),
        (duration, right.value),
    ];
    let mix = |a: (f64, f64), b: (f64, f64), t: f64| (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
    let subdivide = |t: f64| {
        let a = mix(points[0], points[1], t);
        let b = mix(points[1], points[2], t);
        let c = mix(points[2], points[3], t);
        let d = mix(a, b, t);
        let e = mix(b, c, t);
        (a, c, d, e, mix(d, e, t))
    };
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..56 {
        let t = (lo + hi) / 2.0;
        if subdivide(t).4.0 < target {
            lo = t
        } else {
            hi = t
        }
    }
    let (a, c, d, e, _) = subdivide((lo + hi) / 2.0);
    let handle = |dt: f64, dv: f64, bound: R| -> Result<CurveHandle, EditorError> {
        let raw = R::try_from(&scaled(span, dt / duration)?)?;
        let zero = R::new(0, 1)?;
        let value = if bound.ticks() < 0 {
            if raw.cmp_exact(bound).is_lt() {
                bound
            } else if raw.ticks() > 0 {
                zero
            } else {
                raw
            }
        } else if raw.cmp_exact(bound).is_gt() {
            bound
        } else if raw.ticks() < 0 {
            zero
        } else {
            raw
        };
        Ok(CurveHandle {
            dt: value.into(),
            dv,
        })
    };
    let remaining = end.checked_sub(R::try_from(&key.time)?)?;
    left.right_handle = Some(handle(a.0, a.1 - left.value, local)?);
    key.left_handle = Some(handle(
        d.0 - target,
        d.1 - key.value,
        R::new(0, 1)?.checked_sub(local)?,
    )?);
    key.right_handle = Some(handle(e.0 - target, e.1 - key.value, remaining)?);
    right.left_handle = Some(handle(
        c.0 - duration,
        c.1 - right.value,
        R::new(0, 1)?.checked_sub(remaining)?,
    )?);
    key.segment_to_next = ScalarSegment::Bezier;
    Ok(())
}
