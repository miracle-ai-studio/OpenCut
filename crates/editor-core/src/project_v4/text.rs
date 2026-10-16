//! Pure V4 text rules. No I/O or renderer capability claims.
use super::*;

pub(super) fn font_binding(
    value: &FontBindingV4,
    fonts: &HashMap<&str, &FontResourceV4>,
) -> Result<(), EditorError> {
    match value {
        FontBindingV4::Locked { font_id } if fonts.contains_key(font_id.as_str()) => Ok(()),
        FontBindingV4::Locked { .. } => Err(invalid("Locked font reference is missing")),
        FontBindingV4::UnresolvedFamily { family, policy } => {
            required(family, "Unresolved font family")?;
            if let FontResolutionPolicy::ExplicitFallback { families } = policy {
                let mut seen = HashSet::from([family.trim().to_lowercase()]);
                if families.is_empty()
                    || families
                        .iter()
                        .any(|f| f.trim().is_empty() || !seen.insert(f.trim().to_lowercase()))
                {
                    return Err(invalid(
                        "Font fallback families must be distinct, nonempty and ordered",
                    ));
                }
            }
            Ok(())
        }
    }
}

pub(super) fn font_size(value: &FontSizeV4) -> Result<(), EditorError> {
    match value {
        FontSizeV4::ClassicHeight90 { value } | FontSizeV4::CanvasPixels { value } => {
            positive(*value, "Font size")
        }
    }
}

pub(super) fn font_request(
    binding: &FontBindingV4,
    weight: FontWeightV4,
    style: FontStyleV4,
    fonts: &HashMap<&str, &FontResourceV4>,
) -> Result<(), EditorError> {
    font_binding(binding, fonts)?;
    if let FontBindingV4::Locked { font_id } = binding {
        let face = fonts[font_id.as_str()];
        let requested_weight = match weight {
            FontWeightV4::Normal => 400,
            FontWeightV4::Bold => 700,
        };
        if is_font_synthesis_resource_v1(face) {
            validate_font_synthesis_resource_v1(face)?;
            if requested_weight != face.weight || style != FontStyleV4::Italic {
                return Err(invalid(
                    "Font synthesis binding requires an explicit same-weight italic request",
                ));
            }
        } else if face.weight != requested_weight || face.style != style {
            return Err(invalid(
                "Locked font face does not match requested weight and style",
            ));
        }
    }
    Ok(())
}

pub(super) fn text(
    value: &TextV4,
    fonts: &HashMap<&str, &FontResourceV4>,
) -> Result<(), EditorError> {
    font_request(&value.font, value.font_weight, value.font_style, fonts)?;
    font_size(&value.font_size)?;
    color(&value.color)?;
    finite(value.letter_spacing_px, "Letter spacing")?;
    positive(value.line_height_multiplier, "Line height")?;
    color(&value.background.color)?;
    bound(
        value.background.corner_radius_percent,
        0.0,
        100.0,
        "Text corner radius percent",
    )?;
    nonnegative(value.background.padding_x_at_size15, "Text padding X")?;
    nonnegative(value.background.padding_y_at_size15, "Text padding Y")?;
    finite(value.background.offset_x_px, "Text background X")?;
    finite(value.background.offset_y_px, "Text background Y")
}

pub(super) fn caption_validate(
    value: &CaptionV4,
    assets: &HashMap<&str, &AssetV4>,
    fonts: &HashMap<&str, &FontResourceV4>,
    ids: &mut HashSet<String>,
) -> Result<(), EditorError> {
    unique(ids, &value.id)?;
    required(&value.text, "Caption text")?;
    range(&value.timeline_range)?;
    font_binding(&value.style.font, fonts)?;
    if let FontBindingV4::Locked { font_id } = &value.style.font
        && is_font_synthesis_resource_v1(fonts[font_id.as_str()])
    {
        return Err(invalid(
            "CaptionV4 has no requested font style and cannot bind a synthesis font",
        ));
    }
    font_size(&value.style.font_size)?;
    color(&value.style.color)?;
    if let Some(value) = &value.style.background_color {
        color(value)?;
    }
    if value.style.max_width_px == Some(0)
        || value
            .style
            .line_height_px
            .is_some_and(|x| !x.is_finite() || x <= 0.0)
    {
        return Err(invalid("Caption layout dimensions must be positive"));
    }
    if value.style.layout == CaptionLayoutProfileV4::PinnedCaptionV4
        && !matches!(value.style.font, FontBindingV4::Locked { .. })
    {
        return Err(invalid("Pinned caption requires a locked font"));
    }
    if let Some(anchor) = &value.source_anchor {
        let end = range(&anchor.source_range)?;
        let asset = assets
            .get(anchor.asset_id.as_str())
            .copied()
            .ok_or_else(|| invalid("Caption source anchor is missing"))?;
        // Legacy V2 anchors are provenance, including historical silent media.
        // A newly pinned caption must bind an audio-bearing source instead.
        if value.style.layout == CaptionLayoutProfileV4::PinnedCaptionV4 {
            if !matches!(asset.kind, AssetKind::Audio | AssetKind::Video) {
                return Err(invalid("Caption source anchor must be audio-bearing media"));
            }
            if matches!(asset.metadata.audio, AudioPresenceV4::Absent {}) {
                return Err(invalid(
                    "Caption source anchor cannot reference known absent audio",
                ));
            }
        }
        if let Some(value) = &asset.duration
            && end.cmp_exact(duration(value)?) == Ordering::Greater
        {
            return Err(invalid("Caption anchor exceeds resource duration"));
        }
    }
    Ok(())
}
