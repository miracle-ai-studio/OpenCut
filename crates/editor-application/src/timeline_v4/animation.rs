//! Classic keyframes.ts splitScalarChannelAtTime, expressed over typed V4
//! channels. De Casteljau preserves time/value controls and value overshoot;
//! Classic 120000-tick signed rounding applies only to newly authored handles.
use super::*;
use opencut_editor_core::project_v4::{SampledChannelV4, sample_animation_channel_v4};
fn seconds(t: R) -> f64 {
    t.ticks() as f64 / t.timebase() as f64
}
fn point(t: f64, p0: f64, p1: f64, p2: f64, p3: f64) -> f64 {
    let q = 1. - t;
    q * q * q * p0 + 3. * q * q * t * p1 + 3. * q * t * t * p2 + t * t * t * p3
}
fn mix(a: (f64, f64), b: (f64, f64), t: f64) -> (f64, f64) {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}
fn handle(dt: f64, dv: f64, span: R, left: bool) -> Result<CurveHandleV4, EditorError> {
    let ticks = (dt * 120000.).round();
    if !ticks.is_finite() || ticks.abs() >= i64::MAX as f64 || !dv.is_finite() {
        return Err(invalid("Cut curve control exceeds finite range"));
    }
    let rounded = R::new(ticks as i64, 120000)?;
    let magnitude = if left {
        zero().checked_sub(rounded)?
    } else {
        rounded
    };
    let dt = if magnitude.cmp_exact(span).is_gt() {
        if left {
            zero().checked_sub(span)?
        } else {
            span
        }
    } else {
        rounded
    };
    Ok(CurveHandleV4 { dt: dt.into(), dv })
}
fn sample(channel: &AnimationChannelV4, at: R) -> Result<f64, EditorError> {
    match sample_animation_channel_v4(channel, &at.into())? {
        SampledChannelV4::Scalar { raw, .. } => Ok(raw),
        _ => Err(invalid("Expected a scalar channel")),
    }
}
fn boundary(
    channel: &AnimationChannelV4,
    keys: &mut Vec<ScalarKeyV4>,
    at: R,
    ids: &mut identities::Ids<'_>,
    owner: &str,
) -> Result<(), EditorError> {
    if keys
        .iter()
        .any(|k| time(&k.time).is_ok_and(|t| t.cmp_exact(at).is_eq()))
    {
        return Ok(());
    }
    let value = sample(channel, at)?;
    let index =
        keys.partition_point(|k| time(&k.time).expect("validated key").cmp_exact(at).is_lt());
    let mut key = ScalarKeyV4 {
        id: ids.fresh(
            owner,
            "tl_boundary",
            &format!("{}:{}:{}", cid(channel), at.ticks(), at.timebase()),
        )?,
        segment_id: None,
        time: at.into(),
        value,
        left_handle: None,
        right_handle: None,
        segment_to_next: ScalarSegmentV4::Linear,
        tangent_mode: TangentModeV4::Broken,
    };
    if index > 0 && index < keys.len() {
        let (before, after) = keys.split_at_mut(index);
        let left = before.last_mut().unwrap();
        let right = &mut after[0];
        key.segment_to_next = left.segment_to_next.clone();
        match &left.segment_to_next {
            ScalarSegmentV4::Bezier => {
                let start = time(&left.time)?;
                let end = time(&right.time)?;
                let span = end.checked_sub(start)?;
                let duration = seconds(span);
                let delta = right.value - left.value;
                let out = left.right_handle.clone().unwrap_or(CurveHandleV4 {
                    dt: span.checked_scale(R::new(1, 1)?, R::new(3, 1)?)?.into(),
                    dv: delta / 3.,
                });
                let incoming = right.left_handle.clone().unwrap_or(CurveHandleV4 {
                    dt: zero()
                        .checked_sub(span.checked_scale(R::new(1, 1)?, R::new(3, 1)?)?)?
                        .into(),
                    dv: -delta / 3.,
                });
                let p0 = (0., left.value);
                let p1 = (seconds(time(&out.dt)?), left.value + out.dv);
                let p2 = (
                    duration + seconds(time(&incoming.dt)?),
                    right.value + incoming.dv,
                );
                let p3 = (duration, right.value);
                let progress = seconds(at.checked_sub(start)?);
                let (mut lo, mut hi) = (0., 1.);
                for _ in 0..56 {
                    let mid = (lo + hi) * 0.5;
                    if point(mid, p0.0, p1.0, p2.0, p3.0) < progress {
                        lo = mid
                    } else {
                        hi = mid
                    }
                }
                let t = (lo + hi) * 0.5;
                let q0 = mix(p0, p1, t);
                let q1 = mix(p1, p2, t);
                let q2 = mix(p2, p3, t);
                let r0 = mix(q0, q1, t);
                let r1 = mix(q1, q2, t);
                let split = mix(r0, r1, t);
                let left_span = at.checked_sub(start)?;
                let right_span = end.checked_sub(at)?;
                left.right_handle = Some(handle(q0.0, q0.1 - p0.1, left_span, false)?);
                key.left_handle = Some(handle(r0.0 - split.0, r0.1 - split.1, left_span, true)?);
                key.right_handle = Some(handle(r1.0 - split.0, r1.1 - split.1, right_span, false)?);
                right.left_handle = Some(handle(q2.0 - p3.0, q2.1 - p3.1, right_span, true)?);
            }
            ScalarSegmentV4::LegacyNormalizedBezier { .. } => {
                return Err(unsupported(
                    "Editing a legacy lookup curve requires explicit conversion; its frozen sampled profile cannot be silently replaced",
                ));
            }
            ScalarSegmentV4::Step => {
                key.segment_to_next = ScalarSegmentV4::Step;
            }
            ScalarSegmentV4::Linear => {}
        }
    }
    keys.insert(index, key);
    Ok(())
}
fn cid(c: &AnimationChannelV4) -> &str {
    match c {
        AnimationChannelV4::Scalar { id, .. } | AnimationChannelV4::Discrete { id, .. } => id,
    }
}
pub(super) fn crop(
    channels: &[AnimationChannelV4],
    start: R,
    end: R,
    ids: &mut identities::Ids<'_>,
    owner: &str,
) -> Result<Vec<AnimationChannelV4>, EditorError> {
    if !end.cmp_exact(start).is_gt() {
        return Err(invalid("Animation crop must have positive duration"));
    }
    let mut result = Vec::new();
    for channel in channels {
        let mut out = channel.clone();
        match &mut out {
            AnimationChannelV4::Scalar { keys, .. } => {
                boundary(channel, keys, start, ids, owner)?;
                boundary(channel, keys, end, ids, owner)?;
                keys.retain(|k| {
                    let t = time(&k.time).unwrap();
                    !t.cmp_exact(start).is_lt() && !t.cmp_exact(end).is_gt()
                });
                for k in keys {
                    k.time = time(&k.time)?.checked_sub(start)?.into();
                }
            }
            AnimationChannelV4::Discrete { keys, .. } => {
                if !keys
                    .iter()
                    .any(|k| time(&k.time).is_ok_and(|t| t.cmp_exact(start).is_eq()))
                {
                    let SampledChannelV4::Discrete { value, .. } =
                        sample_animation_channel_v4(channel, &start.into())?
                    else {
                        unreachable!()
                    };
                    keys.push(DiscreteKeyV4 {
                        id: ids.fresh(
                            owner,
                            "tl_boundary",
                            &format!("{}:{}:{}", cid(channel), start.ticks(), start.timebase()),
                        )?,
                        time: start.into(),
                        value,
                    });
                }
                keys.retain(|k| {
                    let t = time(&k.time).unwrap();
                    !t.cmp_exact(start).is_lt() && !t.cmp_exact(end).is_gt()
                });
                keys.sort_by(|a, b| time(&a.time).unwrap().cmp_exact(time(&b.time).unwrap()));
                for k in keys {
                    k.time = time(&k.time)?.checked_sub(start)?.into();
                }
            }
        }
        result.push(out);
    }
    Ok(result)
}

/// Classic keyframes.ts clampAnimationsToDuration delegates to the *left* side
/// of splitAnimationsAtTime. It does not rescale the clock or add a start key.
pub(super) fn clamp_duration_classic(
    channels: &[AnimationChannelV4],
    end: R,
    ids: &mut identities::Ids<'_>,
    owner: &str,
) -> Result<Vec<AnimationChannelV4>, EditorError> {
    if end.ticks() <= 0 {
        return Err(invalid("Animation clamp requires positive duration"));
    }
    let mut result = Vec::with_capacity(channels.len());
    for channel in channels {
        let mut out = channel.clone();
        let keep = match &mut out {
            AnimationChannelV4::Scalar { keys, .. } => {
                let boundary_mode = keys
                    .windows(2)
                    .find(|pair| {
                        time(&pair[0].time).unwrap().cmp_exact(end).is_lt()
                            && time(&pair[1].time).unwrap().cmp_exact(end).is_gt()
                    })
                    .map(|pair| {
                        if matches!(pair[0].segment_to_next, ScalarSegmentV4::Bezier) {
                            pair[0].tangent_mode
                        } else {
                            TangentModeV4::Flat
                        }
                    });
                if let Some(mode) = boundary_mode {
                    boundary(channel, keys, end, ids, owner)?;
                    // Classic's newly created boundary keeps Bezier tangents;
                    // non-Bezier boundary keys use its default linear segment.
                    if let Some(last) = keys
                        .iter_mut()
                        .find(|key| time(&key.time).unwrap().cmp_exact(end).is_eq())
                    {
                        last.tangent_mode = mode;
                        if !matches!(last.segment_to_next, ScalarSegmentV4::Bezier) {
                            last.segment_to_next = ScalarSegmentV4::Linear;
                        }
                    }
                }
                keys.retain(|key| !time(&key.time).unwrap().cmp_exact(end).is_gt());
                !keys.is_empty()
            }
            AnimationChannelV4::Discrete { keys, .. } => {
                if !keys
                    .iter()
                    .any(|key| time(&key.time).unwrap().cmp_exact(end).is_eq())
                {
                    let SampledChannelV4::Discrete { value, .. } =
                        sample_animation_channel_v4(channel, &end.into())?
                    else {
                        unreachable!()
                    };
                    keys.push(DiscreteKeyV4 {
                        id: ids.fresh(
                            owner,
                            "tl_boundary",
                            &format!("{}:{}:{}", cid(channel), end.ticks(), end.timebase()),
                        )?,
                        time: end.into(),
                        value,
                    });
                }
                keys.retain(|key| !time(&key.time).unwrap().cmp_exact(end).is_gt());
                keys.sort_by(|a, b| time(&a.time).unwrap().cmp_exact(time(&b.time).unwrap()));
                !keys.is_empty()
            }
        };
        if keep {
            result.push(out);
        }
    }
    Ok(result)
}
