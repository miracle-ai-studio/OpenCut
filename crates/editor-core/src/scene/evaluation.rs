//! Pure animation sampling. Results carry both authored curve output and the
//! versioned parameter registry's physical value; no rendering is implied.
use super::parameters::{apply_sample, validate_stored_discrete, validate_stored_scalar};
use super::*;

mod curves;
mod legacy_conversion;
pub use legacy_conversion::*;
#[cfg(test)]
mod tests;
mod validation;

/// Internal typed sample, deliberately separate from any public wire schema.
#[derive(Clone, Debug, PartialEq)]
pub enum SampledChannel {
    Scalar {
        binding: ScalarBinding,
        raw: f64,
        physical: f64,
    },
    Discrete {
        parameter: DiscreteParameter,
        value: DiscreteValue,
    },
}

/// Exact clock translation for an element's timeline-local channels. Retime and
/// source trims do not participate in this clock. Before-start times are signed.
pub fn element_local_time_scene(
    timeline_time: &Time,
    element_start: &Time,
) -> Result<Time, EditorError> {
    let at = RationalTime::try_from(timeline_time)?;
    let start = RationalTime::try_from(element_start)?;
    let (mut top, mut bottom) = difference(at, start);
    let mut a = top.unsigned_abs();
    let mut b = bottom as u128;
    while b != 0 {
        (a, b) = (b, a % b);
    }
    let divisor = a.max(1) as i128;
    top /= divisor;
    bottom /= divisor;
    Ok(RationalTime::new(
        i64::try_from(top).map_err(|_| invalid("Local clock ticks overflow"))?,
        u32::try_from(bottom).map_err(|_| invalid("Local clock timebase overflows"))?,
    )?
    .into())
}

/// Safely sample one standalone channel. The caller supplies its declared local
/// clock (element-local or sequence-local for a track). Aggregate target ownership
/// and applicability still require validate_project_v4; this entry point checks
/// the entire channel's sampling structure even when sampling an exact endpoint.
pub fn sample_animation_channel_scene(
    channel: &AnimationChannel,
    at: &Time,
) -> Result<SampledChannel, EditorError> {
    let at = RationalTime::try_from(at)?;
    ValidatedAnimationChannel::new(channel)?.sample_at(at)
}

/// Immutable sampling proof for repeated evaluation of one channel. The borrow
/// prevents changing keys after validation; ownership/applicability still belong
/// to aggregate project validation. This is an in-process type, never a DTO.
pub struct ValidatedAnimationChannel<'a> {
    channel: &'a AnimationChannel,
    times: Vec<RationalTime>,
}

impl<'a> ValidatedAnimationChannel<'a> {
    pub fn new(channel: &'a AnimationChannel) -> Result<Self, EditorError> {
        let times = match channel {
            AnimationChannel::Scalar {
                id,
                target,
                binding,
                keys,
                ..
            } => validation::scalar(id, target, binding, keys)?,
            AnimationChannel::Discrete {
                id,
                target,
                parameter,
                keys,
            } => {
                let times = validation::discrete(id, target, keys)?;
                for key in keys {
                    validate_stored_discrete(*parameter, &key.value)?;
                }
                times
            }
        };
        Ok(Self { channel, times })
    }

    pub fn sample(&self, at: &Time) -> Result<SampledChannel, EditorError> {
        self.sample_at(RationalTime::try_from(at)?)
    }

    fn sample_at(&self, at: RationalTime) -> Result<SampledChannel, EditorError> {
        match self.channel {
            AnimationChannel::Scalar {
                binding,
                keys,
                extrapolation,
                ..
            } => {
                let raw = scalar(keys, &self.times, *extrapolation, at)?;
                finite(raw, "Animation sample")?;
                let physical = apply_sample(binding, raw)?;
                Ok(SampledChannel::Scalar {
                    binding: *binding,
                    raw,
                    physical,
                })
            }
            AnimationChannel::Discrete {
                parameter, keys, ..
            } => {
                // Exact boundary belongs to the new key. Before/after hold at edges.
                let index = self
                    .times
                    .partition_point(|time| time.cmp_exact(at) != Ordering::Greater)
                    .saturating_sub(1);
                Ok(SampledChannel::Discrete {
                    parameter: *parameter,
                    value: keys[index].value.clone(),
                })
            }
        }
    }
}

fn scalar(
    keys: &[ScalarKey],
    times: &[RationalTime],
    extrapolation: Extrapolation,
    at: RationalTime,
) -> Result<f64, EditorError> {
    let after = times.partition_point(|time| time.cmp_exact(at) == Ordering::Less);
    if after < times.len() && times[after].cmp_exact(at) == Ordering::Equal {
        return Ok(keys[after].value);
    }
    if keys.len() == 1 {
        return Ok(keys[0].value);
    }
    if after == 0 {
        return Ok(match extrapolation.before {
            ExtrapolationMode::Hold => keys[0].value,
            ExtrapolationMode::Linear => {
                lerp(keys[0].value, keys[1].value, ratio(at, times[0], times[1]))
            }
        });
    }
    if after == times.len() {
        let last = keys.len() - 1;
        return Ok(match extrapolation.after {
            ExtrapolationMode::Hold => keys[last].value,
            ExtrapolationMode::Linear => lerp(
                keys[last - 1].value,
                keys[last].value,
                ratio(at, times[last - 1], times[last]),
            ),
        });
    }
    let left = &keys[after - 1];
    let right = &keys[after];
    let progress = ratio(at, times[after - 1], times[after]);
    match &left.segment_to_next {
        ScalarSegment::Step => Ok(left.value),
        ScalarSegment::Linear => Ok(lerp(left.value, right.value, progress)),
        ScalarSegment::Bezier => {
            curves::actual(left, right, times[after - 1], times[after], progress)
        }
        ScalarSegment::LegacyNormalizedBezier { curve } => Ok(curves::legacy_value(
            left.value,
            right.value,
            curve,
            progress,
        )),
    }
}

fn lerp(left: f64, right: f64, progress: f64) -> f64 {
    left + (right - left) * progress
}

/// i64 ticks times u32 base fit i128. Subtract *before* floating conversion,
/// retaining adjacent ticks near i64::MAX, even across distinct timebases.
fn difference(left: RationalTime, right: RationalTime) -> (i128, i128) {
    (
        i128::from(left.ticks()) * i128::from(right.timebase())
            - i128::from(right.ticks()) * i128::from(left.timebase()),
        i128::from(left.timebase()) * i128::from(right.timebase()),
    )
}
fn ratio(at: RationalTime, left: RationalTime, right: RationalTime) -> f64 {
    let (delta, _) = difference(at, left);
    let (span, _) = difference(right, left);
    (delta as f64 / span as f64) * (f64::from(right.timebase()) / f64::from(at.timebase()))
}

fn offset_ratio(offset: RationalTime, left: RationalTime, right: RationalTime) -> f64 {
    let (span, base) = difference(right, left);
    (offset.ticks() as f64 / span as f64) * (base as f64 / f64::from(offset.timebase()))
}
