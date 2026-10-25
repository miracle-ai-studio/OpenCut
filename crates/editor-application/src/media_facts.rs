//! Shared Classic rate normalization and original historical media identity.
use opencut_contracts::{EditorError, FrameRate};
fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
const STANDARD_IMPORT_FRAME_RATES: [(u32, u32); 10] = [
    (24000, 1001),
    (24, 1),
    (25, 1),
    (30000, 1001),
    (30, 1),
    (48, 1),
    (50, 1),
    (60000, 1001),
    (60, 1),
    (120, 1),
];

/// Normalize an unrounded Classic packet observation, preserving the original
/// floating comparison/tolerance and positive Math.round semantics.
pub fn normalize_classic_packet_fps(rate: f64) -> Result<FrameRate, EditorError> {
    if !rate.is_finite() || rate <= 0.0 || rate > f64::from(u32::MAX) {
        return Err(invalid(
            "Classic packet frame rate is outside the project format",
        ));
    }
    for (n, d) in STANDARD_IMPORT_FRAME_RATES {
        if (rate - f64::from(n) / f64::from(d)).abs() <= 0.01 {
            return Ok(FrameRate {
                numerator: n,
                denominator: d,
            });
        }
    }
    if rate.fract() == 0.0 {
        return Ok(FrameRate {
            numerator: rate as u32,
            denominator: 1,
        });
    }
    let mut n = (rate * 1_000_000.0).round() as u64;
    if n == 0 {
        return Err(invalid("Classic packet frame rate rounds to zero"));
    }
    let mut d = 1_000_000_u64;
    let (mut a, mut b) = (n, d);
    while b != 0 {
        (a, b) = (b, a % b);
    }
    n /= a;
    d /= a;
    Ok(FrameRate {
        numerator: u32::try_from(n)
            .map_err(|_| invalid("Imported frame rate exceeds the project format"))?,
        denominator: d as u32,
    })
}

// Classic fps/utils.ts ordered normalization within 0.01 fps, without floating time.
pub(crate) fn normalize_import_fps(rate: &FrameRate) -> Result<FrameRate, EditorError> {
    if rate.numerator == 0 || rate.denominator == 0 {
        return Err(invalid("Imported frame rate must be positive"));
    }
    for (n, d) in STANDARD_IMPORT_FRAME_RATES {
        let delta = (i128::from(rate.numerator) * i128::from(d)
            - i128::from(n) * i128::from(rate.denominator))
        .abs();
        if delta * 100 <= i128::from(rate.denominator) * i128::from(d) {
            return Ok(FrameRate {
                numerator: n,
                denominator: d as u32,
            });
        }
    }
    // Classic's arbitrary 1e6 denominator and positive Math.round, then reduce.
    let mut n = (u64::from(rate.numerator) * 1_000_000 + u64::from(rate.denominator) / 2)
        / u64::from(rate.denominator);
    let mut d = 1_000_000_u64;
    let (mut a, mut b) = (n, d);
    while b != 0 {
        (a, b) = (b, a % b);
    }
    n /= a;
    d /= a;
    Ok(FrameRate {
        numerator: u32::try_from(n)
            .map_err(|_| invalid("Imported frame rate exceeds the project format"))?,
        denominator: d as u32,
    })
}

/// Original immutable identity recorded by imported historical media. A
/// descriptor is not proof that CAS bytes are presently available.
pub fn legacy_asset_version(
    asset: &opencut_contracts::Asset,
) -> Option<opencut_contracts::AssetVersion> {
    asset.version.clone().or_else(|| {
        let hash = asset.metadata.get("content_hash")?.as_str()?;
        let size = asset.metadata.get("size")?.as_u64()?;
        Some(opencut_contracts::AssetVersion {
            id: format!("av_{hash}"),
            content_hash: hash.into(),
            size,
        })
    })
}

#[cfg(test)]
mod tests;
