//! Shared fixed-source acquisition and retained provenance. Algorithms are
//! selected by approved identity; raw proof formats remain immutable metadata.
use super::*;
use crate::font_catalog::selection::{FontAcquisition, select};
fn not_derived() -> EditorError {
    invalid("The approved source is not a font derivation recipe")
}
pub fn is_provider(provider: &str) -> bool {
    matches!(
        provider,
        crate::font_derived::OUTLINE_FONT_PROVIDER
            | crate::font_named::NAMED_FONT_PROVIDER
            | crate::font_synthesis::FONT_SYNTHESIS_PROVIDER
            | crate::font_color_static::PROVIDER
    )
}
pub fn source(id: &str) -> Result<PinnedDerivedFont, EditorError> {
    match select(id)? {
        FontAcquisition::Outline => crate::font_derived::source(id),
        FontAcquisition::RenamedOutline => crate::font_named::source(id),
        FontAcquisition::CompleteColor => crate::font_color_static::source(id),
        FontAcquisition::Synthesis => crate::font_synthesis::pinned_source(id),
        _ => Err(not_derived()),
    }
}
pub fn validate_import_request(request: &ImportFont) -> Result<(), EditorError> {
    match select(&request.catalog_id)? {
        FontAcquisition::Outline => crate::font_derived::validate_import_request(request),
        FontAcquisition::RenamedOutline => crate::font_named::validate_import_request(request),
        FontAcquisition::CompleteColor => {
            crate::font_color_static::validate_import_request(request)
        }
        FontAcquisition::Synthesis => crate::font_synthesis::validate_import_request(request),
        _ => Err(not_derived()),
    }
}
pub fn expected_import_resource(request: &ImportFont) -> Result<FontResource, EditorError> {
    match select(&request.catalog_id)? {
        FontAcquisition::Outline => crate::font_derived::expected_import_resource(request),
        FontAcquisition::RenamedOutline => crate::font_named::expected_import_resource(request),
        FontAcquisition::CompleteColor => {
            crate::font_color_static::expected_import_resource(request)
        }
        FontAcquisition::Synthesis => crate::font_synthesis::expected_import_resource(request),
        _ => Err(not_derived()),
    }
}
pub fn prepare_derived_font(
    provider: &impl DerivedFontSource,
    inspector: &impl FontInspector,
    request: &ImportFont,
    fetch: &FontFetchPolicy,
    inspection: &FontImportPolicy,
    checkpoint: &mut dyn FontImportCheckpoint,
) -> Result<PreparedDerivedFont, EditorError> {
    match select(&request.catalog_id)? {
        FontAcquisition::Outline => crate::font_derived::prepare_derived_font(
            provider, inspector, request, fetch, inspection, checkpoint,
        ),
        FontAcquisition::RenamedOutline => crate::font_named::prepare_derived_font(
            provider, inspector, request, fetch, inspection, checkpoint,
        ),
        FontAcquisition::CompleteColor => crate::font_color_static::prepare(
            provider, inspector, request, fetch, inspection, checkpoint,
        ),
        FontAcquisition::Synthesis => crate::font_synthesis::prepare_synthesis_font(
            provider, inspector, request, fetch, inspection, checkpoint,
        ),
        _ => Err(not_derived()),
    }
}
pub fn validate_source(source: &FontDerivedSource) -> Result<(), EditorError> {
    let pin = PinnedDerivedFont {
        source: source.clone(),
    };
    let proof: Value = serde_json::from_str(&source.proof_json)
        .map_err(|_| invalid("Invalid portable derivation proof"))?;
    match proof["schema_version"].as_u64() {
        Some(1) => crate::font_derived::validate_pinned_source(&pin),
        Some(2) => {
            crate::font_named::validate_pinned_source(&pin)?;
            if crate::font_named::source(&source.face.id)?.source != *source {
                return Err(invalid(
                    "Portable renamed proof differs from reviewed pinned recipe",
                ));
            }
            Ok(())
        }
        Some(3) => {
            crate::font_synthesis::validate_pinned_source(&pin)?;
            if crate::font_synthesis::pinned_source(&source.face.id)?.source != *source {
                return Err(invalid(
                    "Portable synthesis proof differs from its exact compiled binding",
                ));
            }
            Ok(())
        }
        Some(4) => crate::font_color_static::validate_pinned_source(&pin),
        _ => Err(invalid("Unsupported derivation proof version")),
    }
}
pub fn validate_resource_source(
    font: &FontResource,
    source: &FontDerivedSource,
) -> Result<(), EditorError> {
    match &font.origin {
        ResourceOrigin::Generated { provider, .. }
            if provider == crate::font_color_static::PROVIDER =>
        {
            crate::font_color_static::validate_resource_source(font, source)
        }
        ResourceOrigin::Generated { provider, .. }
            if provider == crate::font_synthesis::FONT_SYNTHESIS_PROVIDER =>
        {
            crate::font_synthesis::validate_resource_source(font, source)
        }
        ResourceOrigin::Generated { provider, .. }
            if provider == crate::font_named::NAMED_FONT_PROVIDER =>
        {
            crate::font_named::proof::validate_resource_source(font, source)
        }
        ResourceOrigin::Generated { provider, .. }
            if provider == crate::font_derived::OUTLINE_FONT_PROVIDER =>
        {
            crate::font_derived::proof::validate_resource_source(font, source)
        }
        _ => Err(invalid("Font provider has no supported derivation proof")),
    }
}

/// The existing fixed-revision proof port carries all retained proof versions.
/// Match both the actual resource provider and its fully validated source; an
/// arbitrary proof JSON version never selects a more permissive validator.
pub fn retained_proof_format(
    font: &FontResource,
    source: &FontDerivedSource,
) -> Result<u32, EditorError> {
    validate_resource_source(font, source)?;
    match &font.origin {
        ResourceOrigin::Generated { provider, .. }
            if provider == crate::font_color_static::PROVIDER =>
        {
            Ok(4)
        }
        ResourceOrigin::Generated { provider, .. }
            if provider == crate::font_derived::OUTLINE_FONT_PROVIDER =>
        {
            Ok(1)
        }
        ResourceOrigin::Generated { provider, .. }
            if provider == crate::font_named::NAMED_FONT_PROVIDER =>
        {
            Ok(2)
        }
        ResourceOrigin::Generated { provider, .. }
            if provider == crate::font_synthesis::FONT_SYNTHESIS_PROVIDER =>
        {
            Ok(3)
        }
        _ => Err(invalid(
            "Font provider has no supported retained proof version",
        )),
    }
}
