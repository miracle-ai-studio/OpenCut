//! Evaluate time/value controls without normalizing away value overshoot.
use super::*;
use opencut_contracts::historical_visual::HistoricalVisualCubicBezier;

fn point(t: f64, p0: f64, p1: f64, p2: f64, p3: f64) -> f64 {
    let inverse = 1.0 - t;
    inverse * inverse * inverse * p0
        + 3.0 * inverse * inverse * t * p1
        + 3.0 * inverse * t * t * p2
        + t * t * t * p3
}

fn solve(progress: f64, x1: f64, x2: f64, iterations: usize) -> f64 {
    let (mut low, mut high) = (0.0, 1.0);
    for _ in 0..iterations {
        let middle = (low + high) * 0.5;
        if point(middle, 0.0, x1, x2, 1.0) < progress {
            low = middle;
        } else {
            high = middle;
        }
    }
    (low + high) * 0.5
}

pub(super) fn actual(
    left: &ScalarKey,
    right: &ScalarKey,
    start: RationalTime,
    end: RationalTime,
    progress: f64,
) -> Result<f64, EditorError> {
    let delta = right.value - left.value;
    // Constant value controls describe exactly one physical value regardless
    // of the time controls. Avoid solving an irrelevant time polynomial for
    // every audio sample. Keep signed-zero evaluation on its original path.
    if left.value != 0.0
        && left.value.to_bits() == right.value.to_bits()
        && left.right_handle.as_ref().is_none_or(|h| h.dv == 0.0)
        && right.left_handle.as_ref().is_none_or(|h| h.dv == 0.0)
    {
        return Ok(left.value);
    }
    // Both omitted controls are exactly the line, avoiding numeric solver noise.
    if left.right_handle.is_none() && right.left_handle.is_none() {
        return Ok(lerp(left.value, right.value, progress));
    }
    let (x1, y1) = match &left.right_handle {
        Some(handle) => (
            offset_ratio(RationalTime::try_from(&handle.dt)?, start, end),
            left.value + handle.dv,
        ),
        None => (1.0 / 3.0, left.value + delta / 3.0),
    };
    let (x2, y2) = match &right.left_handle {
        Some(handle) => (
            1.0 + offset_ratio(RationalTime::try_from(&handle.dt)?, start, end),
            right.value + handle.dv,
        ),
        None => (2.0 / 3.0, right.value - delta / 3.0),
    };
    // The exact structure was already checked. Relative time controls remain in
    // [0,1]; a tiny conversion error at the edge does not change that structure.
    let t = solve(progress, x1.clamp(0.0, 1.0), x2.clamp(0.0, 1.0), 56);
    Ok(point(t, left.value, y1, y2, right.value))
}

fn decimal12(value: f64) -> f64 {
    // The old native expression builder emits fixed-point decimal constants,
    // not a mathematical round(value * 1e12) (which can overflow).
    format!("{value:.12}")
        .parse()
        .expect("finite formatted decimal")
}

/// Frozen old native profile: cubic_bezier_lookup in media/backend.rs uses 32
/// linear lookup segments, 40 solve iterations, then 12-place constants. This
/// deliberately differs from old application split's 48-iteration exact ease.
/// Only curve/value constants are preserved here; clock selection is exact V4.
pub(super) fn legacy_value(
    left: f64,
    right: f64,
    curve: &HistoricalVisualCubicBezier,
    progress: f64,
) -> f64 {
    if left == right {
        return decimal12(left);
    }
    const SUBDIVISIONS: usize = 32;
    let index = ((progress * SUBDIVISIONS as f64).floor() as usize).min(SUBDIVISIONS - 1);
    let x0 = index as f64 / SUBDIVISIONS as f64;
    let x1 = (index + 1) as f64 / SUBDIVISIONS as f64;
    let y0 = point(
        solve(x0, curve.x1, curve.x2, 40),
        0.0,
        curve.y1,
        curve.y2,
        1.0,
    );
    let y1 = point(
        solve(x1, curve.x1, curve.x2, 40),
        0.0,
        curve.y1,
        curve.y2,
        1.0,
    );
    let eased =
        decimal12(y0) + decimal12(y1 - y0) * ((progress - decimal12(x0)) / decimal12(x1 - x0));
    decimal12(left) + decimal12(right - left) * eased
}
