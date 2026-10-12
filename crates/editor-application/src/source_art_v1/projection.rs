use super::*;

// Wider exact fractions are used only to project bounded samples. They never
// rewrite authored Time representations or pass through floating-point seconds.
#[derive(Clone, Copy)]
struct Fraction {
    n: u128,
    d: u128,
}
impl Fraction {
    fn new(n: u128, d: u128) -> Result<Self, EditorError> {
        if d == 0 {
            return Err(overflow());
        }
        let g = gcd(n, d);
        Ok(Self { n: n / g, d: d / g })
    }
    fn time(time: &opencut_contracts::Time) -> Result<Self, EditorError> {
        let value = RationalTime::try_from(time)?;
        if value.ticks() < 0 {
            return Err(invalid("Source-art time must be nonnegative"));
        }
        Self::new(
            u128::from(value.ticks().unsigned_abs()),
            u128::from(value.timebase()),
        )
    }
    fn add(self, other: Self) -> Result<Self, EditorError> {
        let g = gcd(self.d, other.d);
        Self::new(
            self.n
                .checked_mul(other.d / g)
                .and_then(|n| {
                    other
                        .n
                        .checked_mul(self.d / g)
                        .and_then(|r| n.checked_add(r))
                })
                .ok_or_else(overflow)?,
            self.d.checked_mul(other.d / g).ok_or_else(overflow)?,
        )
    }
    fn scale(self, n: u128, d: u128) -> Result<Self, EditorError> {
        if d == 0 {
            return Err(overflow());
        }
        let a = gcd(self.n, d);
        let b = gcd(n, self.d);
        Self::new(
            (self.n / a).checked_mul(n / b).ok_or_else(overflow)?,
            (self.d / b).checked_mul(d / a).ok_or_else(overflow)?,
        )
    }
    fn min(self, other: Self) -> Result<Self, EditorError> {
        let g = gcd(self.d, other.d);
        let left = self.n.checked_mul(other.d / g).ok_or_else(overflow)?;
        let right = other.n.checked_mul(self.d / g).ok_or_else(overflow)?;
        Ok(if left <= right { self } else { other })
    }
    fn sample(self, rate: u32, ceil: bool) -> Result<u64, EditorError> {
        let value = self.scale(u128::from(rate), 1)?;
        let sample = if ceil {
            value.n.div_ceil(value.d)
        } else {
            value.n / value.d
        };
        u64::try_from(sample).map_err(|_| overflow())
    }
}
fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a.max(1)
}

pub(super) fn project(
    prepared: &PreparedSourceArtV1,
    sample_rate: u32,
    total_samples: u64,
    summary: &[f32],
) -> Result<Vec<f32>, EditorError> {
    let SourceArtQueryV1::Waveform { projection, .. } = &prepared.request.query else {
        return Err(receipt(
            "Source waveform projection has a different request kind",
        ));
    };
    let mut result = Vec::new();
    match projection {
        WaveformProjectionV1::Source { range, bins } => {
            let (start, duration) = match range {
                Some(range) => (
                    Fraction::time(&range.start)?,
                    Fraction::time(&range.duration)?,
                ),
                None => (
                    Fraction::new(0, 1)?,
                    Fraction::new(u128::from(total_samples), u128::from(sample_rate))?,
                ),
            };
            result.reserve(*bins as usize);
            for index in 0..*bins {
                let left = start.add(duration.scale(u128::from(index), u128::from(*bins))?)?;
                let right = start.add(duration.scale(u128::from(index + 1), u128::from(*bins))?)?;
                result.push(peak(
                    summary,
                    total_samples,
                    left.sample(sample_rate, false)?,
                    right.sample(sample_rate, true)?,
                )?);
            }
        }
        WaveformProjectionV1::Element {
            local_range,
            bins,
            bin_duration,
            ..
        } => {
            let clock = prepared
                .element_clock
                .as_ref()
                .ok_or_else(|| receipt("Source waveform lost its fixed element clock"))?;
            let local_start = Fraction::time(&local_range.start)?;
            let local_end = local_start.add(Fraction::time(&local_range.duration)?)?;
            let step = Fraction::time(bin_duration)?;
            let source_start = Fraction::new(
                u128::from(clock.source_start.ticks().unsigned_abs()),
                u128::from(clock.source_start.timebase()),
            )?;
            result.reserve(*bins as usize);
            for index in 0..*bins {
                let left = local_start.add(step.scale(u128::from(index), 1)?)?;
                let right = local_start
                    .add(step.scale(u128::from(index + 1), 1)?)?
                    .min(local_end)?;
                let source = |local: Fraction| {
                    source_start.add(local.scale(
                        u128::from(clock.rate.numerator),
                        u128::from(clock.rate.denominator),
                    )?)
                };
                result.push(peak(
                    summary,
                    total_samples,
                    source(left)?.sample(sample_rate, false)?,
                    source(right)?.sample(sample_rate, true)?,
                )?);
            }
        }
    }
    Ok(result)
}

fn peak(summary: &[f32], extent: u64, start: u64, end: u64) -> Result<f32, EditorError> {
    // Classic summary sampling deliberately expands to whole 128-sample
    // buckets; this is an envelope, not a claim of exact raw-sample maxima.
    let end = end.min(extent);
    if end <= start {
        return Ok(0.0);
    }
    let first = usize::try_from(start / 128).map_err(|_| overflow())?;
    let end = usize::try_from(end.div_ceil(128)).map_err(|_| overflow())?;
    let values = summary
        .get(first..end.min(summary.len()))
        .ok_or_else(|| receipt("Waveform summary coverage differs"))?;
    Ok(values.iter().copied().fold(0.0f32, f32::max))
}
