use opencut_contracts::{EditorError, FrameRate, Time, TimeRange};
use std::cmp::Ordering;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RationalTime {
    ticks: i64,
    timebase: u32,
}

impl RationalTime {
    pub fn new(ticks: i64, timebase: u32) -> Result<Self, EditorError> {
        if timebase == 0 {
            return Err(invalid("timebase must be greater than zero"));
        }
        Ok(Self { ticks, timebase })
    }
    pub fn ticks(self) -> i64 {
        self.ticks
    }
    pub fn timebase(self) -> u32 {
        self.timebase
    }
    pub fn checked_add(self, rhs: Self) -> Result<Self, EditorError> {
        arithmetic(self, rhs, true)
    }
    pub fn checked_sub(self, rhs: Self) -> Result<Self, EditorError> {
        arithmetic(self, rhs, false)
    }
    pub fn cmp_exact(self, rhs: Self) -> Ordering {
        (self.ticks as i128 * rhs.timebase as i128)
            .cmp(&(rhs.ticks as i128 * self.timebase as i128))
    }
    pub fn checked_scale(self, numerator: Self, denominator: Self) -> Result<Self, EditorError> {
        if denominator.ticks == 0 {
            return Err(invalid("scale denominator must not be zero"));
        }
        if denominator.ticks < 0 {
            return Err(invalid("scale ratio must be positive"));
        }
        let mut top = [
            self.ticks.unsigned_abs() as u128,
            numerator.ticks.unsigned_abs() as u128,
            denominator.timebase as u128,
        ];
        let mut bottom = [
            self.timebase as u128,
            numerator.timebase as u128,
            denominator.ticks.unsigned_abs() as u128,
        ];
        // Cancel every numerator factor against every denominator factor before
        // multiplication. All inputs fit independently, while their unreduced
        // products need not fit even when the final rational value does.
        for top_factor in &mut top {
            for bottom_factor in &mut bottom {
                let divisor = gcd_u128(*top_factor, *bottom_factor);
                *top_factor /= divisor;
                *bottom_factor /= divisor;
            }
        }
        let magnitude = top.into_iter().try_fold(1u128, |value, factor| {
            value.checked_mul(factor).ok_or_else(overflow)
        })?;
        let bottom = bottom.into_iter().try_fold(1u128, |value, factor| {
            value.checked_mul(factor).ok_or_else(overflow)
        })?;
        let negative = self.ticks.is_negative() != numerator.ticks.is_negative();
        let ticks = if negative {
            if magnitude > (i64::MAX as u128) + 1 {
                return Err(overflow());
            }
            if magnitude == (i64::MAX as u128) + 1 {
                i64::MIN
            } else {
                -(magnitude as i64)
            }
        } else {
            i64::try_from(magnitude).map_err(|_| overflow())?
        };
        let timebase = u32::try_from(bottom).map_err(|_| overflow())?;
        Self::new(ticks, timebase)
    }
}

impl TryFrom<&Time> for RationalTime {
    type Error = EditorError;
    fn try_from(value: &Time) -> Result<Self, Self::Error> {
        let ticks = value
            .ticks
            .parse::<i64>()
            .map_err(|_| invalid("ticks must be a signed 64-bit decimal integer"))?;
        Self::new(ticks, value.timebase)
    }
}

impl From<RationalTime> for Time {
    fn from(value: RationalTime) -> Self {
        Self {
            ticks: value.ticks.to_string(),
            timebase: value.timebase,
        }
    }
}

pub fn validate_range(range: &TimeRange, require_positive: bool) -> Result<(), EditorError> {
    let start = RationalTime::try_from(&range.start)?;
    let duration = RationalTime::try_from(&range.duration)?;
    if duration.ticks < 0 || (require_positive && duration.ticks == 0) {
        return Err(invalid("duration must be positive"));
    }
    start.checked_add(duration)?;
    Ok(())
}

pub fn range_end(range: &TimeRange) -> Result<Time, EditorError> {
    Ok(RationalTime::try_from(&range.start)?
        .checked_add(RationalTime::try_from(&range.duration)?)?
        .into())
}

/// Selection on a sequence's presentation frame grid. Nearest ties select the
/// later frame; Exact rejects times between frames. Negative times are invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameRounding {
    Exact,
    Nearest,
    Floor,
    Ceil,
}

pub fn align_to_frame(
    time: &Time,
    fps: &FrameRate,
    rounding: FrameRounding,
) -> Result<Time, EditorError> {
    let time = RationalTime::try_from(time)?;
    if time.ticks < 0 || fps.numerator == 0 || fps.denominator == 0 {
        return Err(invalid(
            "frame selection requires non-negative time and positive frame rate",
        ));
    }
    // i64 * u32 and u32 * u32 fit i128. Do not convert through floating seconds.
    let numerator = time.ticks as i128 * fps.numerator as i128;
    let denominator = time.timebase as i128 * fps.denominator as i128;
    let index = numerator / denominator;
    let remainder = numerator % denominator;
    let increment = match rounding {
        FrameRounding::Exact if remainder != 0 => {
            return Err(invalid("requested time is not on the sequence frame grid"));
        }
        FrameRounding::Exact | FrameRounding::Floor => false,
        FrameRounding::Ceil => remainder != 0,
        FrameRounding::Nearest => remainder * 2 >= denominator,
    };
    let ticks = (index + i128::from(increment))
        .checked_mul(fps.denominator as i128)
        .ok_or_else(overflow)?;
    let base = fps.numerator as i128;
    let divisor = gcd_i128(ticks, base);
    Ok(RationalTime::new(
        i64::try_from(ticks / divisor).map_err(|_| overflow())?,
        u32::try_from(base / divisor).map_err(|_| overflow())?,
    )?
    .into())
}

fn arithmetic(a: RationalTime, b: RationalTime, add: bool) -> Result<RationalTime, EditorError> {
    let gcd = gcd(a.timebase, b.timebase);
    let base = (a.timebase as u64 / gcd as u64)
        .checked_mul(b.timebase as u64)
        .ok_or_else(overflow)?;
    if base > u32::MAX as u64 {
        return Err(overflow());
    }
    let left = a.ticks as i128 * (base / a.timebase as u64) as i128;
    let right = b.ticks as i128 * (base / b.timebase as u64) as i128;
    let ticks = if add {
        left.checked_add(right)
    } else {
        left.checked_sub(right)
    }
    .ok_or_else(overflow)?;
    let ticks = i64::try_from(ticks).map_err(|_| overflow())?;
    Ok(RationalTime {
        ticks,
        timebase: base as u32,
    })
}
fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a
}
fn gcd_i128(mut a: i128, mut b: i128) -> i128 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a
}
fn gcd_u128(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a.max(1)
}
fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
fn overflow() -> EditorError {
    EditorError::new(
        "TIME_OVERFLOW",
        "exact time result is outside the public representation",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compares_across_timebases_exactly() {
        assert_eq!(
            RationalTime::new(1, 24)
                .unwrap()
                .cmp_exact(RationalTime::new(1000, 24000).unwrap()),
            Ordering::Equal
        );
    }
    #[test]
    fn adds_without_rounding() {
        let v = RationalTime::new(1, 3)
            .unwrap()
            .checked_add(RationalTime::new(1, 6).unwrap())
            .unwrap();
        assert_eq!((v.ticks(), v.timebase()), (3, 6));
    }

    #[test]
    fn scale_cancels_extreme_equal_factors_before_multiplication() {
        let value = RationalTime::new(i64::MAX, 1).unwrap();
        let same_ratio = RationalTime::new(i64::MAX, u32::MAX).unwrap();
        assert_eq!(value.checked_scale(same_ratio, same_ratio).unwrap(), value);

        let cross_cancelled = RationalTime::new(i64::MAX, u32::MAX)
            .unwrap()
            .checked_scale(
                RationalTime::new(u32::MAX as i64, 1).unwrap(),
                RationalTime::new(i64::MAX, 1).unwrap(),
            )
            .unwrap();
        assert_eq!(cross_cancelled, RationalTime::new(1, 1).unwrap());
    }

    #[test]
    fn scale_preserves_existing_zero_and_sign_behavior() {
        let signed = RationalTime::new(-9, 7)
            .unwrap()
            .checked_scale(
                RationalTime::new(-14, 3).unwrap(),
                RationalTime::new(6, 5).unwrap(),
            )
            .unwrap();
        assert_eq!(signed, RationalTime::new(5, 1).unwrap());
        assert_eq!(
            RationalTime::new(7, 11)
                .unwrap()
                .checked_scale(
                    RationalTime::new(0, 3).unwrap(),
                    RationalTime::new(5, 9).unwrap(),
                )
                .unwrap(),
            RationalTime::new(0, 1).unwrap()
        );
        let negative_denominator = RationalTime::new(1, 1)
            .unwrap()
            .checked_scale(
                RationalTime::new(1, 1).unwrap(),
                RationalTime::new(-1, 1).unwrap(),
            )
            .unwrap_err();
        assert_eq!(negative_denominator.code, "INVALID_ARGUMENT");
    }

    #[test]
    fn scale_reports_only_true_public_representation_overflow() {
        let ticks = RationalTime::new(i64::MAX, 1)
            .unwrap()
            .checked_scale(
                RationalTime::new(2, 1).unwrap(),
                RationalTime::new(1, 1).unwrap(),
            )
            .unwrap_err();
        assert_eq!(ticks.code, "TIME_OVERFLOW");
        let timebase = RationalTime::new(1, u32::MAX)
            .unwrap()
            .checked_scale(
                RationalTime::new(1, u32::MAX).unwrap(),
                RationalTime::new(1, 1).unwrap(),
            )
            .unwrap_err();
        assert_eq!(timebase.code, "TIME_OVERFLOW");
        let zero_denominator = RationalTime::new(1, 1)
            .unwrap()
            .checked_scale(
                RationalTime::new(1, 1).unwrap(),
                RationalTime::new(0, 1).unwrap(),
            )
            .unwrap_err();
        assert_eq!(zero_denominator.code, "INVALID_ARGUMENT");
        assert_eq!(
            zero_denominator.message,
            "scale denominator must not be zero"
        );
    }
    #[test]
    fn rejects_invalid_and_overflowing_values() {
        assert!(
            RationalTime::try_from(&Time {
                ticks: "x".into(),
                timebase: 1
            })
            .is_err()
        );
        assert!(
            RationalTime::new(i64::MAX, 1)
                .unwrap()
                .checked_add(RationalTime::new(1, 1).unwrap())
                .is_err()
        );
    }

    #[test]
    fn frame_alignment_uses_exact_ntsc_grid_and_later_tie() {
        let fps = FrameRate {
            numerator: 30_000,
            denominator: 1001,
        };
        let halfway = Time {
            ticks: "1001".into(),
            timebase: 60_000,
        };
        let zero = Time {
            ticks: "0".into(),
            timebase: 1,
        };
        let first = Time {
            ticks: "1001".into(),
            timebase: 30_000,
        };
        assert_eq!(
            align_to_frame(&halfway, &fps, FrameRounding::Floor).unwrap(),
            zero
        );
        assert_eq!(
            align_to_frame(&halfway, &fps, FrameRounding::Ceil).unwrap(),
            first
        );
        assert_eq!(
            align_to_frame(&halfway, &fps, FrameRounding::Nearest).unwrap(),
            first
        );
        assert!(align_to_frame(&halfway, &fps, FrameRounding::Exact).is_err());
        assert_eq!(
            align_to_frame(&first, &fps, FrameRounding::Exact).unwrap(),
            first
        );
    }

    #[test]
    fn frame_alignment_keeps_ticks_beyond_javascript_integer_precision() {
        let time = Time {
            ticks: "9007199254740995".into(),
            timebase: 24,
        };
        let fps = FrameRate {
            numerator: 24,
            denominator: 1,
        };
        assert_eq!(
            align_to_frame(&time, &fps, FrameRounding::Exact).unwrap(),
            time
        );
        let negative = Time {
            ticks: "-1".into(),
            timebase: 24,
        };
        assert!(align_to_frame(&negative, &fps, FrameRounding::Floor).is_err());
        let invalid_fps = FrameRate {
            numerator: 0,
            denominator: 1,
        };
        assert!(align_to_frame(&time, &invalid_fps, FrameRounding::Nearest).is_err());
        let max = Time {
            ticks: i64::MAX.to_string(),
            timebase: 1,
        };
        let rate = FrameRate {
            numerator: 1,
            denominator: 2,
        };
        assert_eq!(
            align_to_frame(&max, &rate, FrameRounding::Ceil)
                .unwrap_err()
                .code,
            "TIME_OVERFLOW"
        );
    }
}
