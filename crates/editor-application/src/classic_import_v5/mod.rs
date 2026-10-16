//! Classic source requests remain intact. Only compiled, fully proven aliases
//! and explicit Chromium synthesis may differ from the actual native font face.
use crate::classic_import_v4::{
    self, ClassicImportResourcesV4, FontMappingProfile, PreparedClassicImportV4,
};
use opencut_api_contracts::{
    classic_import_v4::ClassicFontRequestV4, classic_import_v5::ClassicAnalyzeV5,
};
use opencut_contracts::{AssetVersion, EditorError, project_v4::*};

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
    request: &ClassicFontRequestV4,
    font: &FontResourceV4,
) -> Result<(), EditorError> {
    let weight = match request.font_weight {
        FontWeightV4::Normal => 400,
        FontWeightV4::Bold => 700,
    };
    if let Some(binding) = crate::font_synthesis_v1::binding_for_resource(font)? {
        let display =
            crate::font_synthesis_v1::display_family_for_resource(font)?.ok_or_else(mismatch)?;
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
    if let ResourceOriginV4::Generated {
        provider,
        generation_id,
    } = &font.origin
        && provider == crate::font_derived_v2::DERIVED_FONT_PROVIDER_V2
    {
        let pin = crate::font_derived_v2::compiled_sources_v2()?
            .iter()
            .find(|pin| &pin.source.face.derivation_id == generation_id)
            .ok_or_else(mismatch)?;
        crate::font_derived_v2::proof::validate_resource_source(font, &pin.source)?;
        return if request.family == font.family
            || request.family == crate::font_derived_v2::display_family(&pin.source.face.id)?
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
    for entry in crate::font_catalog_v3::static_sources_v3() {
        let pin = &entry.approved.source;
        if entry.display_family != request.family
            || pin.content_hash != font.content.content_hash
            || pin.size_bytes != font.content.size
        {
            continue;
        }
        let spec = pin.import_spec(font.id.clone(), String::new());
        let expected = FontResourceV4 {
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

pub fn analyze(
    input: &ClassicAnalyzeV5,
    resources: &ClassicImportResourcesV4,
) -> Result<PreparedClassicImportV4, EditorError> {
    // Include unused mappings: an invalid generated source cannot enter an
    // otherwise empty project by avoiding the text-element resolution path.
    for resolved in &resources.fonts {
        validate_font_mapping(&resolved.request, &resolved.font)?;
    }
    classic_import_v4::analyze_with_profile(input, resources, FontMappingProfile::ApprovedV5)
}
