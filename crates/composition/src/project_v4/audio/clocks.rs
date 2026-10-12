use super::*;

pub(super) enum Rounding {
    Floor,
    Ceil,
    Nearest,
}
pub(super) fn sample_index(
    t: RationalTime,
    rate: u32,
    rounding: Rounding,
) -> Result<u64, CompositionError> {
    if t.ticks() < 0 {
        return Err(invalid("Audio sample clock must be nonnegative"));
    }
    let n = u128::from(t.ticks().unsigned_abs()) * u128::from(rate);
    let d = u128::from(t.timebase());
    let q = match rounding {
        Rounding::Floor => n / d,
        Rounding::Ceil => n.div_ceil(d),
        Rounding::Nearest => (n + d / 2) / d,
    };
    u64::try_from(q).map_err(|_| invalid("Audio sample index overflows u64"))
}
pub(super) fn sample_time(sample: u64, rate: u32, classic: bool) -> Result<Time, CompositionError> {
    if classic {
        let n = u128::from(sample) * 120_000;
        let ticks = (n + u128::from(rate) / 2) / u128::from(rate);
        return Ok(RationalTime::new(
            i64::try_from(ticks).map_err(|_| invalid("Audio animation clock overflows i64"))?,
            120_000,
        )
        .map_err(error)?
        .into());
    }
    let divisor = gcd(u128::from(sample), u128::from(rate));
    Ok(RationalTime::new(
        i64::try_from(u128::from(sample) / divisor)
            .map_err(|_| invalid("Audio animation clock overflows i64"))?,
        u32::try_from(u128::from(rate) / divisor)
            .map_err(|_| invalid("Audio animation timebase overflow"))?,
    )
    .map_err(error)?
    .into())
}
pub(super) fn legacy_rate(rate: RationalRateV4) -> Result<u32, CompositionError> {
    let n = u128::from(rate.numerator);
    let d = u128::from(rate.denominator);
    if n * 2 < d || n > d * 2 {
        return Err(unsupported("Frozen legacy audio rate requires 0.5..2"));
    }
    u32::try_from((n * 48_000 + d / 2) / d)
        .map_err(|_| invalid("Legacy retime sample rate overflow"))
}
fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a.max(1)
}
fn multiply(
    mut n: u128,
    mut d: u128,
    top: u128,
    bottom: u128,
) -> Result<(u128, u128), CompositionError> {
    if d == 0 || bottom == 0 {
        return Err(invalid("Audio rational denominator must be positive"));
    }
    let a = gcd(n, bottom);
    n /= a;
    let b = gcd(top, d);
    d /= b;
    Ok((
        n.checked_mul(top / b)
            .ok_or_else(|| invalid("Source rational numerator overflow"))?,
        d.checked_mul(bottom / a)
            .ok_or_else(|| invalid("Source rational denominator overflow"))?,
    ))
}
pub(super) fn position(
    mapping: &AudioSourceMappingV4,
    sample: u64,
    rate: u32,
) -> Result<ExactSourceSampleV4, CompositionError> {
    if mapping.legacy_retime_sample_rate.is_some() {
        return Err(unsupported(
            "Legacy FFmpeg resampling is not Classic linear interpolation",
        ));
    }
    if rate == 0
        || mapping.output_sample_rate == 0
        || mapping.effective_rate.numerator == 0
        || mapping.effective_rate.denominator == 0
    {
        return Err(invalid("PCM rate must be positive"));
    }
    let start = time(&mapping.clock.nominal_request.start)?;
    if start.ticks() < 0 {
        return Err(invalid("Audio source start must be nonnegative"));
    }
    let (mut n, mut d) = multiply(
        u128::from(start.ticks().unsigned_abs()),
        u128::from(start.timebase()),
        u128::from(rate),
        1,
    )?;
    let (ln, ld) = multiply(
        u128::from(sample),
        u128::from(mapping.output_sample_rate),
        u128::from(mapping.effective_rate.numerator),
        u128::from(mapping.effective_rate.denominator),
    )?;
    let (ln, ld) = multiply(ln, ld, u128::from(rate), 1)?;
    let g = gcd(d, ld);
    n = n
        .checked_mul(ld / g)
        .and_then(|left| {
            ln.checked_mul(d / g)
                .and_then(|right| left.checked_add(right))
        })
        .ok_or_else(|| invalid("Source rational position overflow"))?;
    d = d
        .checked_mul(ld / g)
        .ok_or_else(|| invalid("Source rational denominator overflow"))?;
    let g = gcd(n, d);
    n /= g;
    d /= g;
    Ok(ExactSourceSampleV4 {
        lower: u64::try_from(n / d).map_err(|_| invalid("Source sample index overflows u64"))?,
        fraction_numerator: n % d,
        fraction_denominator: d,
    })
}
