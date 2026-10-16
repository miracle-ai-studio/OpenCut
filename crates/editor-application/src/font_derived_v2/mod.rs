//! Reserved-name-safe derived instances with real opaque native identities.
pub use crate::font_derived_v1::{DerivedFontSourceV1, PinnedDerivedFontV1, PreparedDerivedFontV1};
use crate::font_import_v1::catalog::FontFetchPolicyV1;
use crate::font_import_v1::*;
use opencut_contracts::{AssetVersion, EditorError, font_derived_v1::*, project_v4::*};
use serde_json::Value;
use sha2::{Digest, Sha256};
mod approved;
pub mod compat;
mod corrections;
mod large;
mod names;
pub mod proof;
#[cfg(test)]
mod synthesis_compat_tests;
#[cfg(test)]
mod tests;
mod validation;
pub use validation::validate_pinned_source_v2;
use validation::{invalid, size, text};
pub const DERIVED_FONT_PROVIDER_V2: &str = "opencut-fonttools-static-renamed-v2";
struct Catalog {
    pins: Vec<PinnedDerivedFontV1>,
    index: std::collections::BTreeMap<String, usize>,
}
fn pinned_catalog() -> Result<&'static Catalog, EditorError> {
    static CATALOG: std::sync::OnceLock<Result<Catalog, EditorError>> = std::sync::OnceLock::new();
    CATALOG
        .get_or_init(|| {
            let sources: Vec<FontDerivedSourceV1> =
                serde_json::from_str(include_str!("catalog_pins.json"))
                    .map_err(|_| invalid("Compiled derived font catalog is invalid"))?;
            let mut index = std::collections::BTreeMap::new();
            let mut pins = Vec::with_capacity(sources.len());
            for source in sources {
                let pin = PinnedDerivedFontV1 { source };
                validate_pinned_source_v2(&pin)?;
                if index
                    .insert(pin.source.face.id.clone(), pins.len())
                    .is_some()
                {
                    return Err(invalid("Duplicate derived catalog id"));
                }
                pins.push(pin)
            }
            Ok(Catalog { pins, index })
        })
        .as_ref()
        .map_err(Clone::clone)
}
pub fn catalog() -> Result<Vec<PinnedDerivedFontV1>, EditorError> {
    Ok(pinned_catalog()?.pins.clone())
}
pub fn source(id: &str) -> Result<PinnedDerivedFontV1, EditorError> {
    let catalog = pinned_catalog()?;
    catalog
        .index
        .get(id)
        .map(|index| catalog.pins[*index].clone())
        .ok_or_else(|| EditorError::new("NOT_FOUND", "Unknown pinned derived font recipe"))
}
pub fn validate_import_request_v2(request: &ImportDerivedFontV1) -> Result<(), EditorError> {
    crate::require_idempotency_key(&request.idempotency_key)?;
    if [
        &request.project_id,
        &request.expected_revision,
        &request.font_id,
        &request.catalog_id,
    ]
    .iter()
    .any(|v| !text(v, 1024))
    {
        return Err(invalid("Derived font request identities are invalid"));
    }
    let pin = source(&request.catalog_id)?;
    validate_font_envelope_v1(
        size(&pin.source.face.size_bytes)?,
        &request.required_text,
        &FontImportPolicyV1::default(),
    )
}
pub fn expected_import_resource_v2(
    request: &ImportDerivedFontV1,
) -> Result<FontResourceV4, EditorError> {
    validate_import_request_v2(request)?;
    let s = source(&request.catalog_id)?.source;
    Ok(FontResourceV4 {
        id: request.font_id.clone(),
        family: s.face.family,
        weight: s.face.weight,
        style: s.face.style,
        face_index: 0,
        content: AssetVersion {
            id: format!("av_{}", s.face.content_hash),
            content_hash: s.face.content_hash,
            size: size(&s.face.size_bytes)?,
        },
        origin: ResourceOriginV4::Generated {
            provider: DERIVED_FONT_PROVIDER_V2.into(),
            generation_id: s.face.derivation_id,
        },
        license: Some(ResourceLicenseV4 {
            identifier: Some(s.license_id),
            source_url: Some(s.license_url),
            notice: Some(s.license_notice),
            attribution: None,
        }),
    })
}
pub fn prepare_derived_font_v2(
    provider: &impl DerivedFontSourceV1,
    inspector: &impl FontInspectorV1,
    request: &ImportDerivedFontV1,
    fetch_policy: &FontFetchPolicyV1,
    inspection_policy: &FontImportPolicyV1,
    checkpoint: &mut dyn FontImportCheckpointV1,
) -> Result<PreparedDerivedFontV1, EditorError> {
    checkpoint.check()?;
    let expected = expected_import_resource_v2(request)?;
    let pin = source(&request.catalog_id)?;
    if fetch_policy.max_bytes == 0
        || fetch_policy.max_bytes > large::MAX_SOURCE_BYTES
        || fetch_policy.deadline_ms == 0
        || fetch_policy.deadline_ms > 120000
        || expected.content.size > fetch_policy.max_bytes
    {
        return Err(invalid(
            "Derived source fetch policy exceeds bounded envelope",
        ));
    }
    validate_font_envelope_v1(
        expected.content.size,
        &request.required_text,
        inspection_policy,
    )?;
    let bytes = provider.obtain(&pin, fetch_policy, checkpoint)?;
    checkpoint.check()?;
    validate_font_input_v1(&bytes, &request.required_text, inspection_policy)?;
    let mut digest = Sha256::new();
    for chunk in bytes.chunks(65536) {
        checkpoint.check()?;
        digest.update(chunk)
    }
    if bytes.len() as u64 != expected.content.size
        || format!("{:x}", digest.finalize()) != expected.content.content_hash
    {
        return Err(EditorError::new(
            "FONT_SOURCE_INTEGRITY_ERROR",
            "Derived output differs from pinned proof hash/size",
        ));
    }
    let parsed = inspector.inspect(
        &bytes,
        &request.required_text,
        inspection_policy,
        checkpoint,
    )?;
    if parsed.family != expected.family
        || parsed.weight != expected.weight
        || parsed.style != expected.style
    {
        return Err(EditorError::new(
            "FONT_FACE_MISMATCH",
            "Derived static face metadata differs from its pinned recipe",
        ));
    }
    if parsed.glyph_count == 0 {
        return Err(EditorError::new(
            "INVALID_FONT",
            "Derived font contains no glyphs",
        ));
    }
    checkpoint.check()?;
    Ok(PreparedDerivedFontV1 {
        font: PreparedFontV1 {
            resource: expected,
            glyph_count: parsed.glyph_count,
            checked_codepoints: parsed.checked_codepoints,
        },
        bytes,
    })
}

/// Classic catalog label; FontResource.family retains the real derivative name
/// (opaque OFL/Apache identity or original-family Ubuntu derivative identity).
pub fn display_family(id: &str) -> Result<String, EditorError> {
    let pin = source(id)?;
    let proof: Value = serde_json::from_str(&pin.source.proof_json)
        .map_err(|_| invalid("Invalid compiled renamed proof"))?;
    proof["source"]["source_family"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| invalid("Source display family missing"))
}
pub fn compiled_sources_v2() -> Result<&'static [PinnedDerivedFontV1], EditorError> {
    Ok(&pinned_catalog()?.pins)
}
pub fn render_profile(id: &str) -> Result<String, EditorError> {
    let pin = source(id)?;
    let proof: Value = serde_json::from_str(&pin.source.proof_json)
        .map_err(|_| invalid("Invalid compiled renamed proof"))?;
    proof["source"]["render_profile"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| invalid("Render profile missing"))
}
