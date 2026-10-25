use super::*;
fn js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}
pub(crate) fn verify_wrap(
    original: &str,
    max_width: f64,
    measurements: &[CaptionWrapMeasurement],
) -> Result<String, EditorError> {
    if !max_width.is_finite() || max_width <= 0.0 {
        return Err(invalid("Caption wrap width is invalid"));
    }
    verify_classic_wrap(original, max_width, measurements)
}
/// Classic explicit margins may consume the entire width; its word wrapper
/// then keeps every oversized word intact. The frozen V3 entry still rejects 0.
pub(crate) fn verify_classic_wrap(
    original: &str,
    max_width: f64,
    measurements: &[CaptionWrapMeasurement],
) -> Result<String, EditorError> {
    if !max_width.is_finite() || max_width < 0. {
        return Err(invalid("Caption wrap width is invalid"));
    }
    let mut index = 0;
    let normalized = original.trim_matches(js_space).replace("\r\n", "\n");
    let mut paragraphs = Vec::new();
    for paragraph in normalized.split('\n') {
        let words: Vec<_> = paragraph
            .trim_matches(js_space)
            .split(js_space)
            .filter(|word| !word.is_empty())
            .collect();
        if words.is_empty() {
            paragraphs.push(String::new());
            continue;
        }
        let mut current = words[0].to_string();
        let mut lines = Vec::new();
        for word in &words[1..] {
            let candidate = format!("{current} {word}");
            let measured = measurements
                .get(index)
                .ok_or_else(|| invalid("Caption greedy candidate measurements incomplete"))?;
            index += 1;
            if measured.content != candidate || !measured.width.is_finite() || measured.width < 0.0
            {
                return Err(invalid(
                    "Caption greedy candidate differs from original token order",
                ));
            }
            if measured.width <= max_width {
                current = candidate;
            } else {
                lines.push(current);
                current = word.to_string();
            }
        }
        lines.push(current);
        paragraphs.push(lines.join("\n"));
    }
    let wrapped = paragraphs.join("\n");
    // Final line measures bind glyph widths used by both wrapping and placement.
    for line in wrapped.split('\n') {
        let measured = measurements
            .get(index)
            .ok_or_else(|| invalid("Caption line measurement missing"))?;
        index += 1;
        if measured.content != line || !measured.width.is_finite() || measured.width < 0.0 {
            return Err(invalid("Caption final line measurement changed"));
        }
    }
    if index != measurements.len() {
        return Err(invalid("Unexpected caption measurement"));
    }
    let mut widths = BTreeMap::new();
    for measured in measurements {
        if let Some(previous) = widths.insert(&measured.content, measured.width) {
            if previous != measured.width {
                return Err(invalid("Repeated caption measurement changed width"));
            }
        }
    }
    Ok(wrapped)
}
pub(crate) fn verify_line_widths(
    wrapped: &str,
    measurements: &[CaptionWrapMeasurement],
    lines: &[TextLineMetrics],
) -> Result<(), EditorError> {
    let texts: Vec<_> = wrapped.split('\n').collect();
    if texts.len() != lines.len() {
        return Err(invalid("Caption raster line count changed"));
    }
    let final_metrics = &measurements[measurements.len().saturating_sub(texts.len())..];
    for ((text, measurement), line) in texts.into_iter().zip(final_metrics).zip(lines) {
        if line.content != text || line.width != measurement.width {
            return Err(invalid("Caption raster differs from measured greedy lines"));
        }
    }
    Ok(())
}
