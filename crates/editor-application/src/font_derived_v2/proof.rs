//! V2 fixed-revision proof reader; old V1 reader remains version-strict.
use super::*;
use crate::font_derived_v1::proof::DerivedFontProofReadV1;
pub fn validate_resource_source(
    font: &FontResourceV4,
    source: &FontDerivedSourceV1,
) -> Result<(), EditorError> {
    validate_pinned_source_v2(&PinnedDerivedFontV1 {
        source: source.clone(),
    })?;
    compat::validate_source(source)?;
    if font.family != source.face.family
        || font.weight != source.face.weight
        || font.style != source.face.style
        || font.face_index != 0
        || font.content.content_hash != source.face.content_hash
        || font.content.size != size(&source.face.size_bytes)?
        || font.content.id != format!("av_{}", source.face.content_hash)
        || font.origin
            != (ResourceOriginV4::Generated {
                provider: DERIVED_FONT_PROVIDER_V2.into(),
                generation_id: source.face.derivation_id.clone(),
            })
        || font.license
            != Some(ResourceLicenseV4 {
                identifier: Some(source.license_id.clone()),
                source_url: Some(source.license_url.clone()),
                notice: Some(source.license_notice.clone()),
                attribution: None,
            })
    {
        return Err(EditorError::new(
            "FONT_SOURCE_INTEGRITY_ERROR",
            "Fixed renamed font differs from reviewed source/output/name/license proof",
        ));
    }
    Ok(())
}
pub fn read(
    store: &impl DerivedFontProofReadV1,
    request: &ReadDerivedFontProofV1,
) -> Result<DerivedFontProofV1, EditorError> {
    if [&request.project_id, &request.revision, &request.font_id]
        .iter()
        .any(|v| !text(v, 1024))
    {
        return Err(invalid("Proof requires a fixed project/revision/font"));
    }
    let result = store.read_derived_font_proof_v1(request)?;
    if result.schema_version != 2
        || result.project_id != request.project_id
        || result.revision != request.revision
        || result.font_id != request.font_id
    {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Renamed proof scope/version differs",
        ));
    }
    validate_pinned_source_v2(&PinnedDerivedFontV1 {
        source: result.source.clone(),
    })?;
    compat::validate_source(&result.source)?;
    Ok(result)
}
