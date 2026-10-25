//! Pure V4 timing rules. No I/O or renderer capability claims.
use super::*;

pub(super) fn mapped_duration(
    value: RationalTime,
    retime: &Retime,
    inverse: bool,
) -> Result<RationalTime, EditorError> {
    if retime.rate.numerator == 0 || retime.rate.denominator == 0 {
        return Err(invalid("Retime rate must be positive"));
    }
    let (n, d) = if inverse {
        (retime.rate.denominator, retime.rate.numerator)
    } else {
        (retime.rate.numerator, retime.rate.denominator)
    };
    match retime.mapping {
        RetimeMapping::ExactRational => value
            .checked_scale(
                RationalTime::new(i64::from(n), 1)?,
                RationalTime::new(i64::from(d), 1)?,
            )
            .map_err(|e| invalid(e.message)),
        RetimeMapping::ClassicRoundedTicks { timebase } => {
            if timebase == 0 {
                return Err(invalid("Retime rounding timebase must be positive"));
            }
            let top = i128::from(value.ticks()) * i128::from(n) * i128::from(timebase);
            let bottom = i128::from(value.timebase()) * i128::from(d);
            let rounded = top / bottom + i128::from((top % bottom) * 2 >= bottom);
            RationalTime::new(
                i64::try_from(rounded).map_err(|_| invalid("Rounded source span overflows"))?,
                timebase,
            )
        }
    }
}

pub(super) fn checked_sum(a: RationalTime, b: RationalTime) -> Result<RationalTime, EditorError> {
    a.checked_add(b).map_err(|e| invalid(e.message))
}

pub(super) fn checked_difference(
    a: RationalTime,
    b: RationalTime,
) -> Result<RationalTime, EditorError> {
    a.checked_sub(b).map_err(|e| invalid(e.message))
}

pub(super) fn nonnegative_time(value: &Time) -> Result<RationalTime, EditorError> {
    let value = time(value)?;
    if value.ticks() < 0 {
        return Err(invalid("Source trims must be nonnegative"));
    }
    Ok(value)
}

/// Resolve the quantized source request span, not a decoder/CAS read window.
/// A valid Classic inverse-rounded rate edit can request one more source tick
/// than its declared extent. IR/native consumers must derive an independently
/// bounded read window and explicit EOF behavior; this helper supplies neither.
pub(super) fn element_source(
    value: &TimelineElement,
    retime: Option<&Retime>,
    asset: Option<&ProjectAsset>,
) -> Result<TimeRange, EditorError> {
    let identity = Retime {
        rate: RationalRate {
            numerator: 1,
            denominator: 1,
        },
        maintain_pitch: false,
        mapping: RetimeMapping::ExactRational,
    };
    let rate = retime.unwrap_or(&identity);
    let timeline = duration(&value.timeline_range.duration)?;
    let visible = mapped_duration(timeline, rate, false)?;
    if visible.ticks() <= 0 {
        return Err(invalid("Rounded source span must remain positive"));
    }
    let resolved = match &value.source_span {
        ElementSourceSpan::LegacySourceRange { range: source } => {
            range(source)?;
            if retime.is_some() && duration(&source.duration)?.cmp_exact(visible) != Ordering::Equal
            {
                return Err(invalid(
                    "Legacy source and timeline durations disagree with retime",
                ));
            }
            if let Some(asset_duration) = asset.and_then(|a| a.duration.as_ref())
                && range(source)?.cmp_exact(duration(asset_duration)?) == Ordering::Greater
            {
                return Err(invalid("Source range exceeds resource duration"));
            }
            source.clone()
        }
        ElementSourceSpan::ClassicTrimmed {
            trim_start,
            trim_end,
            source_duration,
        } => {
            let start = nonnegative_time(trim_start)?;
            let tail = nonnegative_time(trim_end)?;
            let visible_end = checked_sum(start, visible)?;
            let full = checked_sum(visible_end, tail)?;
            if let Some(extent) = source_duration {
                let extent = duration(extent)?;
                let available = checked_difference(checked_difference(extent, start)?, tail)?;
                if available.ticks() <= 0 {
                    return Err(invalid("Source trims exhaust its declared extent"));
                }
                // Rate edits round the inverse; splits/resizes round the forward
                // span. These operations are not exact inverses at half ticks.
                if full.cmp_exact(extent) != Ordering::Equal
                    && mapped_duration(available, rate, true)?.cmp_exact(timeline)
                        != Ordering::Equal
                {
                    return Err(invalid(
                        "Classic source extent disagrees with both declared rounding directions",
                    ));
                }
                if let Some(asset_duration) = asset.and_then(|a| a.duration.as_ref())
                    && extent.cmp_exact(duration(asset_duration)?) == Ordering::Greater
                {
                    return Err(invalid("Declared source extent exceeds resource duration"));
                }
            } else if let Some(asset_duration) = asset.and_then(|a| a.duration.as_ref())
                && full.cmp_exact(duration(asset_duration)?) == Ordering::Greater
            {
                return Err(invalid("Inferred source extent exceeds resource duration"));
            }
            TimeRange {
                start: start.into(),
                duration: visible.into(),
            }
        }
    };
    Ok(resolved)
}
