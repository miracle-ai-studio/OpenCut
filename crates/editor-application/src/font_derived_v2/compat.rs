//! Private dispatch shared by recovery, history and package envelopes. Public V1
//! source/import/proof entry points retain V1-only admission.
use super::*;
pub fn is_provider(provider: &str) -> bool {
    matches!(
        provider,
        crate::font_derived_v1::DERIVED_FONT_PROVIDER_V1
            | DERIVED_FONT_PROVIDER_V2
            | crate::font_synthesis_v1::FONT_SYNTHESIS_PROVIDER_V1
    )
}
pub fn source(id: &str) -> Result<PinnedDerivedFontV1, EditorError> {
    if id.ends_with("-synthetic-italic-v1") {
        crate::font_synthesis_v1::pinned_source(id)
    } else if id.ends_with("-derived-v2") {
        super::source(id)
    } else {
        crate::font_derived_v1::source(id)
    }
}
pub fn validate_import_request(request: &ImportDerivedFontV1) -> Result<(), EditorError> {
    if request.catalog_id.ends_with("-synthetic-italic-v1") {
        crate::font_synthesis_v1::validate_import_request(request)
    } else if request.catalog_id.ends_with("-derived-v2") {
        super::validate_import_request_v2(request)
    } else {
        crate::font_derived_v1::validate_import_request_v1(request)
    }
}
pub fn expected_import_resource(
    request: &ImportDerivedFontV1,
) -> Result<FontResourceV4, EditorError> {
    if request.catalog_id.ends_with("-synthetic-italic-v1") {
        crate::font_synthesis_v1::expected_import_resource(request)
    } else if request.catalog_id.ends_with("-derived-v2") {
        super::expected_import_resource_v2(request)
    } else {
        crate::font_derived_v1::expected_import_resource_v1(request)
    }
}
pub fn prepare_derived_font(
    provider: &impl DerivedFontSourceV1,
    inspector: &impl FontInspectorV1,
    request: &ImportDerivedFontV1,
    fetch: &FontFetchPolicyV1,
    inspection: &FontImportPolicyV1,
    checkpoint: &mut dyn FontImportCheckpointV1,
) -> Result<PreparedDerivedFontV1, EditorError> {
    if request.catalog_id.ends_with("-synthetic-italic-v1") {
        crate::font_synthesis_v1::prepare_synthesis_font(
            provider, inspector, request, fetch, inspection, checkpoint,
        )
    } else if request.catalog_id.ends_with("-derived-v2") {
        super::prepare_derived_font_v2(provider, inspector, request, fetch, inspection, checkpoint)
    } else {
        crate::font_derived_v1::prepare_derived_font_v1(
            provider, inspector, request, fetch, inspection, checkpoint,
        )
    }
}
pub fn validate_source(source: &FontDerivedSourceV1) -> Result<(), EditorError> {
    let pin = PinnedDerivedFontV1 {
        source: source.clone(),
    };
    let proof: Value = serde_json::from_str(&source.proof_json)
        .map_err(|_| invalid("Invalid portable derivation proof"))?;
    match proof["schema_version"].as_u64() {
        Some(1) => crate::font_derived_v1::validate_pinned_source_v1(&pin),
        Some(2) => {
            super::validate_pinned_source_v2(&pin)?;
            if super::source(&source.face.id)?.source != *source {
                return Err(invalid(
                    "Portable renamed proof differs from reviewed pinned recipe",
                ));
            }
            Ok(())
        }
        Some(3) => {
            crate::font_synthesis_v1::validate_pinned_source_v1(&pin)?;
            if crate::font_synthesis_v1::pinned_source(&source.face.id)?.source != *source {
                return Err(invalid(
                    "Portable synthesis proof differs from its exact compiled binding",
                ));
            }
            Ok(())
        }
        _ => Err(invalid("Unsupported derivation proof version")),
    }
}
pub fn validate_resource_source(
    font: &FontResourceV4,
    source: &FontDerivedSourceV1,
) -> Result<(), EditorError> {
    match &font.origin {
        ResourceOriginV4::Generated { provider, .. }
            if provider == crate::font_synthesis_v1::FONT_SYNTHESIS_PROVIDER_V1 =>
        {
            crate::font_synthesis_v1::validate_resource_source(font, source)
        }
        ResourceOriginV4::Generated { provider, .. } if provider == DERIVED_FONT_PROVIDER_V2 => {
            super::proof::validate_resource_source(font, source)
        }
        ResourceOriginV4::Generated { provider, .. }
            if provider == crate::font_derived_v1::DERIVED_FONT_PROVIDER_V1 =>
        {
            crate::font_derived_v1::proof::validate_resource_source(font, source)
        }
        _ => Err(invalid("Font provider has no supported derivation proof")),
    }
}

/// The existing fixed-revision proof port carries all retained proof versions.
/// Match both the actual resource provider and its fully validated source; an
/// arbitrary proof JSON version never selects a more permissive validator.
pub fn proof_schema_version(
    font: &FontResourceV4,
    source: &FontDerivedSourceV1,
) -> Result<u32, EditorError> {
    validate_resource_source(font, source)?;
    match &font.origin {
        ResourceOriginV4::Generated { provider, .. }
            if provider == crate::font_derived_v1::DERIVED_FONT_PROVIDER_V1 =>
        {
            Ok(1)
        }
        ResourceOriginV4::Generated { provider, .. } if provider == DERIVED_FONT_PROVIDER_V2 => {
            Ok(2)
        }
        ResourceOriginV4::Generated { provider, .. }
            if provider == crate::font_synthesis_v1::FONT_SYNTHESIS_PROVIDER_V1 =>
        {
            Ok(3)
        }
        _ => Err(invalid(
            "Font provider has no supported retained proof version",
        )),
    }
}
