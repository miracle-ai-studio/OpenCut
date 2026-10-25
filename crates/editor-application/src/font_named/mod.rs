//! Reserved-name-safe derived instances with real opaque native identities.
pub use crate::font_derived::{DerivedFontSource, PinnedDerivedFont, PreparedDerivedFont};
use crate::font_import::catalog::FontFetchPolicy;
use crate::font_import::*;
use opencut_contracts::{AssetVersion, EditorError, font_derived::*, scene::*};
use serde_json::Value;
use sha2::{Digest, Sha256};
mod approved;
mod corrections;
mod large;
mod names;
pub mod proof;
#[cfg(test)]
mod synthesis_compat_tests;
#[cfg(test)]
mod tests;
mod validation;
pub(crate) use validation::canonical;
pub use validation::validate_pinned_source;
use validation::{invalid, size, text};
pub const NAMED_FONT_PROVIDER: &str = "opencut-fonttools-static-renamed-v2";
struct Catalog {
    pins: Vec<PinnedDerivedFont>,
    index: std::collections::BTreeMap<String, usize>,
}
fn pinned_catalog() -> Result<&'static Catalog, EditorError> {
    static CATALOG: std::sync::OnceLock<Result<Catalog, EditorError>> = std::sync::OnceLock::new();
    CATALOG
        .get_or_init(|| {
            let sources: Vec<FontDerivedSource> =
                serde_json::from_str(include_str!("catalog_pins.json"))
                    .map_err(|_| invalid("Compiled derived font catalog is invalid"))?;
            let mut index = std::collections::BTreeMap::new();
            let mut pins = Vec::with_capacity(sources.len());
            for source in sources {
                let pin = PinnedDerivedFont { source };
                validate_pinned_source(&pin)?;
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
pub fn catalog() -> Result<Vec<PinnedDerivedFont>, EditorError> {
    Ok(pinned_catalog()?.pins.clone())
}
pub(crate) fn contains_source(id: &str) -> Result<bool, EditorError> {
    Ok(pinned_catalog()?.index.contains_key(id))
}
pub fn source(id: &str) -> Result<PinnedDerivedFont, EditorError> {
    let catalog = pinned_catalog()?;
    catalog
        .index
        .get(id)
        .map(|index| catalog.pins[*index].clone())
        .ok_or_else(|| EditorError::new("NOT_FOUND", "Unknown pinned derived font recipe"))
}
pub fn validate_import_request(request: &ImportFont) -> Result<(), EditorError> {
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
    validate_font_envelope(
        size(&pin.source.face.size_bytes)?,
        &request.required_text,
        &FontImportPolicy::default(),
    )
}
pub fn expected_import_resource(request: &ImportFont) -> Result<FontResource, EditorError> {
    validate_import_request(request)?;
    let s = source(&request.catalog_id)?.source;
    Ok(FontResource {
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
        origin: ResourceOrigin::Generated {
            provider: NAMED_FONT_PROVIDER.into(),
            generation_id: s.face.derivation_id,
        },
        license: Some(ResourceLicense {
            identifier: Some(s.license_id),
            source_url: Some(s.license_url),
            notice: Some(s.license_notice),
            attribution: None,
        }),
    })
}
pub fn prepare_derived_font(
    provider: &impl DerivedFontSource,
    inspector: &impl FontInspector,
    request: &ImportFont,
    fetch_policy: &FontFetchPolicy,
    inspection_policy: &FontImportPolicy,
    checkpoint: &mut dyn FontImportCheckpoint,
) -> Result<PreparedDerivedFont, EditorError> {
    checkpoint.check()?;
    let expected = expected_import_resource(request)?;
    let pin = source(&request.catalog_id)?;
    prepare_pinned_font(
        provider,
        inspector,
        request,
        &pin,
        expected,
        fetch_policy,
        inspection_policy,
        checkpoint,
    )
}
pub(crate) fn prepare_pinned_font(
    provider: &impl DerivedFontSource,
    inspector: &impl FontInspector,
    request: &ImportFont,
    pin: &PinnedDerivedFont,
    expected: FontResource,
    fetch_policy: &FontFetchPolicy,
    inspection_policy: &FontImportPolicy,
    checkpoint: &mut dyn FontImportCheckpoint,
) -> Result<PreparedDerivedFont, EditorError> {
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
    validate_font_envelope(
        expected.content.size,
        &request.required_text,
        inspection_policy,
    )?;
    let bytes = provider.obtain(pin, fetch_policy, checkpoint)?;
    checkpoint.check()?;
    validate_font_input(&bytes, &request.required_text, inspection_policy)?;
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
    Ok(PreparedDerivedFont {
        font: PreparedFont {
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
pub fn compiled_sources() -> Result<&'static [PinnedDerivedFont], EditorError> {
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
