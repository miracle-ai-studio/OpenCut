//! Fixed-revision retained proofs expose actual source and requested style.
use super::*;
use crate::font_derived_v1::proof::DerivedFontProofReadV1;

pub fn read(
    store: &impl DerivedFontProofReadV1,
    request: &ReadDerivedFontProofV1,
) -> Result<FontSynthesisProofV1, EditorError> {
    if [&request.project_id, &request.revision, &request.font_id]
        .iter()
        .any(|value| !text(value, 1024))
    {
        return Err(invalid(
            "Synthesis proof requires bounded fixed project/revision/font identities",
        ));
    }
    let result = store.read_derived_font_proof_v1(request)?;
    if result.schema_version != 3
        || result.project_id != request.project_id
        || result.revision != request.revision
        || result.font_id != request.font_id
    {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Synthesis proof scope/version differs",
        ));
    }
    validate_pinned_source_v1(&PinnedDerivedFontV1 {
        source: result.source.clone(),
    })?;
    Ok(FontSynthesisProofV1 {
        schema_version: 1,
        project_id: result.project_id,
        revision: result.revision,
        font_id: result.font_id,
        source: source(&result.source.face.id)?,
    })
}
