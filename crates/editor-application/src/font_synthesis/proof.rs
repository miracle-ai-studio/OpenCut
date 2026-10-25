//! Fixed-revision retained proofs expose actual source and requested style.
use super::*;
use crate::font_derived::proof::DerivedFontProofRead;

pub fn read(
    store: &impl DerivedFontProofRead,
    request: &ReadDerivedFontProof,
) -> Result<FontSynthesisProof, EditorError> {
    if [&request.project_id, &request.revision, &request.font_id]
        .iter()
        .any(|value| !text(value, 1024))
    {
        return Err(invalid(
            "Synthesis proof requires bounded fixed project/revision/font identities",
        ));
    }
    let result = store.read_derived_font_proof(request)?;
    if result.project_id != request.project_id
        || result.revision != request.revision
        || result.font_id != request.font_id
    {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Synthesis proof scope differs",
        ));
    }
    validate_pinned_source(&PinnedDerivedFont {
        source: result.source.clone(),
    })?;
    Ok(FontSynthesisProof {
        project_id: result.project_id,
        revision: result.revision,
        font_id: result.font_id,
        source: source(&result.source.face.id)?,
    })
}
