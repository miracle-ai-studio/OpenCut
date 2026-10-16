//! Explicit bounded conversion; the frozen legacy sampler remains unchanged.
use super::*;

pub const LEGACY_LOOKUP_CONVERSION_PROFILE_V1: &str = "opencut-legacy-decimal12-lookup32-linear-v1";
#[derive(Debug)]
pub struct LegacyLookupConversionV1 {
    pub channel: AnimationChannelV4,
    pub converted_segments: u32,
    pub inserted_keys: u32,
    pub max_absolute_error: f64,
    pub error_bound: f64,
}
fn refused(message: &str) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}

/// Convert only requested outgoing segments. Authored endpoints and all other
/// segments are retained. Linear extrapolation at a converted boundary is
/// rejected because V4 derives its outside slope from the adjacent two keys.
pub fn convert_legacy_lookup_segments_v1(
    channel: &AnimationChannelV4,
    left_key_ids: &[String],
    reserved_ids: &HashSet<String>,
    maximum_absolute_error: f64,
) -> Result<LegacyLookupConversionV1, EditorError> {
    if !maximum_absolute_error.is_finite() || maximum_absolute_error < 0.0 {
        return Err(refused(
            "Conversion tolerance must be finite and nonnegative",
        ));
    }
    let AnimationChannelV4::Scalar {
        id,
        target,
        binding,
        keys,
        extrapolation,
    } = channel
    else {
        return Err(refused("Legacy conversion requires a scalar channel"));
    };
    let times = validation::scalar(id, target, binding, keys)?;
    let requested: HashSet<String> = left_key_ids.iter().cloned().collect();
    if requested.is_empty() || requested.len() != left_key_ids.len() || requested.len() > 128 {
        return Err(refused("Choose 1..128 distinct legacy segments"));
    }
    if keys
        .len()
        .saturating_add(requested.len().saturating_mul(31))
        > 4096
    {
        return Err(refused("Conversion exceeds the atomic 4096-key budget"));
    }
    let mut used = reserved_ids.clone();
    used.insert(id.clone());
    used.extend(keys.iter().map(|k| k.id.clone()));
    used.extend(keys.iter().filter_map(|k| k.segment_id.clone()));
    let mut next_id = 0usize;
    let mut output = Vec::with_capacity(keys.len() + 31 * requested.len());
    let mut converted = 0u32;
    let mut maximum = 0.0_f64;
    let mut allowance = 0.0_f64;
    for (index, key) in keys.iter().enumerate() {
        if !requested.contains(&key.id) {
            output.push(key.clone());
            continue;
        }
        let Some(right) = keys.get(index + 1) else {
            return Err(refused("Last key has no outgoing segment"));
        };
        let ScalarSegmentV4::LegacyNormalizedBezier { curve } = &key.segment_to_next else {
            return Err(refused("Every selected segment must use the legacy lookup"));
        };
        if (index == 0 && extrapolation.before == ExtrapolationModeV4::Linear)
            || (index + 2 == keys.len() && extrapolation.after == ExtrapolationModeV4::Linear)
        {
            return Err(refused(
                "Boundary conversion would change linear extrapolation; explicitly choose Hold before converting",
            ));
        }
        let mut values = [0.0; 33];
        values[0] = key.value;
        values[32] = right.value;
        for (i, value) in values.iter_mut().enumerate().take(32).skip(1) {
            *value = curves::legacy_value(key.value, right.value, curve, i as f64 / 32.0);
            finite(*value, "Converted legacy key")?;
            validate_stored_scalar(binding, *value)?;
        }
        // Each original interval is affine after decimal-12 constants. The
        // supremum of the affine difference is at one of its two boundaries.
        // Read the right boundary from its left side to include lookup jumps.
        for i in 0..32 {
            let start = i as f64 / 32.0;
            let end = (i + 1) as f64 / 32.0;
            let before = f64::from_bits(end.to_bits() - 1);
            maximum = maximum.max(
                (curves::legacy_value(key.value, right.value, curve, start) - values[i]).abs(),
            );
            maximum = maximum.max(
                (curves::legacy_value(key.value, right.value, curve, before) - values[i + 1]).abs(),
            );
        }
        // Exact rational knots avoid clock rounding. For finite f64 samples,
        // allow 4096 ulps at the largest endpoint scale for rational-to-f64
        // conversion, local normalization, and the 32x interval amplification.
        // The allowance intentionally remains nonzero even for flat segments.
        let scale = key.value.abs().max(right.value.abs()).max(1.0);
        allowance = allowance.max(scale * (4096.0 * f64::EPSILON));
        let mut outgoing = key.clone();
        outgoing.right_handle = None;
        outgoing.segment_to_next = ScalarSegmentV4::Linear;
        output.push(outgoing);
        let duration = times[index + 1].checked_sub(times[index])?;
        for (i, value) in values.iter().enumerate().take(32).skip(1) {
            let fraction = duration
                .checked_scale(RationalTime::new(i as i64, 32)?, RationalTime::new(1, 1)?)?;
            let time = times[index].checked_add(fraction)?;
            let generated = loop {
                let candidate = format!("legacy_lookup_v1_{next_id}");
                next_id += 1;
                if used.insert(candidate.clone()) {
                    break candidate;
                }
            };
            output.push(ScalarKeyV4 {
                id: generated,
                segment_id: None,
                time: time.into(),
                value: *value,
                left_handle: None,
                right_handle: None,
                segment_to_next: ScalarSegmentV4::Linear,
                tangent_mode: TangentModeV4::Broken,
            });
        }
        converted += 1;
    }
    if converted as usize != requested.len() {
        return Err(refused("A selected legacy segment does not exist"));
    }
    // Clear only incoming handles of converted spans. Adjacent Bezier spans
    // keep the opposite handle, original segment IDs, values and exact times.
    for (index, key) in keys.iter().enumerate().skip(1) {
        if requested.contains(&keys[index - 1].id) {
            if let Some(saved) = output.iter_mut().find(|k| k.id == key.id) {
                saved.left_handle = None;
            }
        }
    }
    let bound = maximum + allowance;
    if !bound.is_finite() || bound > maximum_absolute_error {
        return Err(refused(
            "Legacy lookup conversion exceeds the requested absolute-error tolerance",
        ));
    }
    validation::scalar(id, target, binding, &output)?;
    Ok(LegacyLookupConversionV1 {
        channel: AnimationChannelV4::Scalar {
            id: id.clone(),
            target: target.clone(),
            binding: *binding,
            keys: output,
            extrapolation: *extrapolation,
        },
        converted_segments: converted,
        inserted_keys: 31 * converted,
        max_absolute_error: maximum,
        error_bound: bound,
    })
}

#[cfg(test)]
mod tests;
