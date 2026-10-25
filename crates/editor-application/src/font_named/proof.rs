//! V2 fixed-revision proof reader; old V1 reader remains version-strict.
use super::*;
use crate::font_derived::proof::DerivedFontProofRead;
pub fn validate_resource_source(
    font: &FontResource,
    source: &FontDerivedSource,
) -> Result<(), EditorError> {
    validate_pinned_source(&PinnedDerivedFont {
        source: source.clone(),
    })?;
    crate::font_derived::provenance::validate_source(source)?;
    if font.family != source.face.family
        || font.weight != source.face.weight
        || font.style != source.face.style
        || font.face_index != 0
        || font.content.content_hash != source.face.content_hash
        || font.content.size != size(&source.face.size_bytes)?
        || font.content.id != format!("av_{}", source.face.content_hash)
        || font.origin
            != (ResourceOrigin::Generated {
                provider: NAMED_FONT_PROVIDER.into(),
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
            "Fixed renamed font differs from reviewed source/output/name/license proof",
        ));
    }
    Ok(())
}
