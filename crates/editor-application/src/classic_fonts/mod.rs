//! Classic source requests remain intact. Only compiled, fully proven aliases
//! and explicit Chromium synthesis may differ from the actual native font face.
use opencut_api_contracts::classic_restore::ClassicFontRequest;
use opencut_contracts::{AssetVersion, EditorError, scene::*};

#[cfg(test)]
mod tests;

fn mismatch() -> EditorError {
    EditorError::new(
        "FONT_FACE_MISMATCH",
        "Fixed font is not an approved binding for the original Classic family/weight/style request",
    )
}

/// The request is the source document's authored intent; the resource keeps its
/// real parsed family/style and complete provenance. A matching hash alone is
/// not an alias grant. This function performs no fetch or glyph-coverage claim.
pub fn validate_font_mapping(
    request: &ClassicFontRequest,
    font: &FontResource,
) -> Result<(), EditorError> {
    let weight = match request.font_weight {
        FontWeight::Normal => 400,
        FontWeight::Bold => 700,
    };
    if let Some(binding) = crate::font_synthesis::binding_for_resource(font)? {
        let display =
            crate::font_synthesis::display_family_for_resource(font)?.ok_or_else(mismatch)?;
        return if (request.family == binding.actual.family || request.family == display)
            && weight == binding.requested.weight
            && request.font_style == binding.requested.style
        {
            Ok(())
        } else {
            Err(mismatch())
        };
    }
    if weight != font.weight || request.font_style != font.style {
        return Err(mismatch());
    }
    if let ResourceOrigin::Generated {
        provider,
        generation_id,
    } = &font.origin
        && provider == crate::font_color_static::PROVIDER
    {
        let pin = crate::font_color_static::compiled_sources()?
            .iter()
            .find(|pin| &pin.source.face.derivation_id == generation_id)
            .ok_or_else(mismatch)?;
        crate::font_color_static::validate_resource_source(font, &pin.source)?;
        return if request.family == font.family
            || request.family == crate::font_color_static::display_family(&pin.source.face.id)?
        {
            Ok(())
        } else {
            Err(mismatch())
        };
    }
    if let ResourceOrigin::Generated {
        provider,
        generation_id,
    } = &font.origin
        && provider == crate::font_named::NAMED_FONT_PROVIDER
    {
        let pin = crate::font_named::compiled_sources()?
            .iter()
            .find(|pin| &pin.source.face.derivation_id == generation_id)
            .ok_or_else(mismatch)?;
        crate::font_named::proof::validate_resource_source(font, &pin.source)?;
        return if request.family == font.family
            || request.family == crate::font_named::display_family(&pin.source.face.id)?
        {
            Ok(())
        } else {
            Err(mismatch())
        };
    }
    // The exact legacy mapping is still valid. No new alias authority is needed.
    if request.family == font.family {
        return Ok(());
    }
    for entry in crate::font_catalog::static_sources() {
        let pin = &entry.approved.source;
        if entry.display_family != request.family
            || pin.content_hash != font.content.content_hash
            || pin.size_bytes != font.content.size
        {
            continue;
        }
        let spec = pin.import_spec(font.id.clone(), String::new());
        let expected = FontResource {
            id: spec.font_id,
            family: spec.family,
            weight: spec.weight,
            style: spec.style,
            face_index: 0,
            content: AssetVersion {
                id: format!("av_{}", pin.content_hash),
                content_hash: pin.content_hash.into(),
                size: pin.size_bytes,
            },
            origin: spec.origin,
            license: Some(spec.license),
        };
        if font == &expected {
            return Ok(());
        }
    }
    Err(mismatch())
}
