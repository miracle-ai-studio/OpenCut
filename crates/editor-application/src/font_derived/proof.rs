//! Fixed-revision proof reads never resolve a mutable head or native locator.
use super::*;
pub trait DerivedFontProofRead {
    fn read_derived_font_proof(
        &self,
        request: &ReadDerivedFontProof,
    ) -> Result<DerivedFontProof, EditorError>;
}
pub fn validate_resource_source(
    font: &FontResource,
    source: &FontDerivedSource,
) -> Result<(), EditorError> {
    validate_pinned_source(&PinnedDerivedFont {
        source: source.clone(),
    })?;
    if font.family != source.face.family
        || font.weight != source.face.weight
        || font.style != source.face.style
        || font.face_index != 0
        || font.content.content_hash != source.face.content_hash
        || font.content.size != size(&source.face.size_bytes)?
        || font.content.id != format!("av_{}", source.face.content_hash)
        || font.origin
            != (ResourceOrigin::Generated {
                provider: OUTLINE_FONT_PROVIDER.into(),
                generation_id: source.face.derivation_id.clone(),
            })
        || font.license
            != Some(ResourceLicense {
                identifier: Some(source.license_id.clone()),
                source_url: Some(source.license_url.clone()),
                notice: Some(source.license_notice.clone()),
                attribution: None,
            })
    {
        return Err(EditorError::new(
            "FONT_SOURCE_INTEGRITY_ERROR",
            "Fixed derived font differs from retained derivation source/output/license",
        ));
    }
    Ok(())
}
/// Read the original authorized retained proof through one source-independent port.
pub fn read(
    store: &impl crate::font_derived::proof::DerivedFontProofRead,
    request: &opencut_contracts::font_derived::ReadDerivedFontProof,
) -> Result<opencut_contracts::font_derived::DerivedFontProof, EditorError> {
    for value in [&request.project_id, &request.revision, &request.font_id] {
        if value.trim().is_empty() || value.len() > 1024 || value.chars().any(char::is_control) {
            return Err(EditorError::new(
                "INVALID_ARGUMENT",
                "Font proof requires bounded fixed scope",
            ));
        }
    }
    let result = store.read_derived_font_proof(request)?;
    if result.project_id != request.project_id
        || result.revision != request.revision
        || result.font_id != request.font_id
    {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Retained font proof scope differs",
        ));
    }
    crate::font_derived::provenance::validate_source(&result.source)?;
    if !crate::font_catalog::selection::select(&result.source.face.id)?.derived()
        || crate::font_derived::provenance::source(&result.source.face.id)?.source != result.source
    {
        return Err(EditorError::new(
            "FONT_SOURCE_INTEGRITY_ERROR",
            "Retained font source differs from its approved identity",
        ));
    }
    Ok(result)
}
