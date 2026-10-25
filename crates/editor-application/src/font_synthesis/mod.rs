//! Reviewed normal source plus an explicit, versioned Chromium italic request.
//! This is a rendering binding, not an outline derivation or a new native face.
use crate::{
    font_derived::{DerivedFontSource, PinnedDerivedFont, PreparedDerivedFont},
    font_import::{
        catalog::{FontFetchPolicy, PinnedFontSource, pinned_font_source},
        *,
    },
};
pub use opencut_contracts::font_synthesis::{
    FONT_SYNTHESIS_BINDING_ID_PREFIX, FONT_SYNTHESIS_PROVIDER,
};
use opencut_contracts::{
    AssetVersion, EditorError, font_derived::*, font_import::FontCatalogEntry, font_synthesis::*,
    scene::*,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

mod catalog;
pub use catalog::{
    SynthesisAcquisitionLimits, SynthesisBaseSource, acquisition_limits, base_priority,
    base_source, display_family, render_profile,
};
pub mod proof;
#[cfg(test)]
mod tests;

// Deliberately finite. Adding a source requires review; a known provider string
// or a self-consistent caller-authored proof does not authorize another source.
const ACTUAL_SOURCE_IDS: &[&str] = &[
    "google-bonanova-bold-v1",
    "google-bonanovasc-bold-v1",
    "google-cardo-bold-v1",
    "google-gudea-bold-v1",
    "google-judson-bold-v1",
    "google-lekton-bold-v1",
    "google-libertinussans-bold-v1",
    "google-neuton-bold-v1",
    "google-oldstandardtt-bold-v1",
    "google-trochut-bold-v1",
    "google-zain-bold-v1",
];

fn invalid(message: &str) -> EditorError {
    EditorError::new("FONT_SYNTHESIS_INVALID", message)
}
fn text(value: &str, limit: usize) -> bool {
    !value.trim().is_empty() && value.len() <= limit && !value.chars().any(char::is_control)
}
fn canonical(value: &Value) -> Result<String, EditorError> {
    fn sorted(value: &Value) -> Value {
        match value {
            Value::Object(map) => {
                let mut pairs: Vec<_> = map.iter().collect();
                pairs.sort_by(|a, b| a.0.cmp(b.0));
                Value::Object(
                    pairs
                        .into_iter()
                        .map(|(k, v)| (k.clone(), sorted(v)))
                        .collect(),
                )
            }
            Value::Array(values) => Value::Array(values.iter().map(sorted).collect()),
            _ => value.clone(),
        }
    }
    serde_json::to_string(&sorted(value)).map_err(|_| invalid("Invalid synthesis proof"))
}
fn from_actual(actual: &PinnedFontSource) -> Result<FontSynthesisSource, EditorError> {
    if actual.style != FontStyle::Normal
        || !matches!(actual.weight, 400 | 700)
        || actual.size_bytes == 0
        || actual.size_bytes > MAX_FONT_BYTES
        || format!("{:x}", Sha256::digest(actual.license_notice.as_bytes()))
            != actual.license_sha256
    {
        return Err(invalid(
            "Compiled synthesis source metadata or license is invalid",
        ));
    }
    let stem = actual
        .catalog_id
        .strip_suffix("-v1")
        .ok_or_else(|| invalid("Compiled source has no versioned catalog identity"))?;
    let id = format!("{stem}-synthetic-italic-v1");
    let entry = FontCatalogEntry {
        id: actual.catalog_id.into(),
        family: actual.family.into(),
        weight: actual.weight,
        style: actual.style,
        source_url: actual.source_url.into(),
        source_commit: actual.source_commit.into(),
        content_hash: actual.content_hash.into(),
        size_bytes: actual.size_bytes.to_string(),
        license_id: actual.license_identifier.into(),
        license_notice: actual.license_notice.into(),
    };
    let requested = FontSynthesisRequest {
        weight: actual.weight,
        style: FontStyle::Italic,
    };
    let profile = FontSynthesisProfile::ClassicChromiumItalicV1;
    let mut proof = json!({
        "schema_version": 3,
        "kind": "browser_font_synthesis",
        "provider": FONT_SYNTHESIS_PROVIDER,
        "profile": profile,
        "catalog_id": id,
        "actual": entry,
        "requested": requested,
        "license_url": actual.license_url,
        "license_sha256": actual.license_sha256,
        "attribution": actual.attribution,
        "source_bytes_unchanged": true,
    });
    let binding_id = format!(
        "{FONT_SYNTHESIS_BINDING_ID_PREFIX}{:x}",
        Sha256::digest(canonical(&proof)?.as_bytes())
    );
    proof["derivation_id"] = Value::String(binding_id.clone());
    Ok(FontSynthesisSource {
        source: FontDerivedSource {
            face: FontDerivedFace {
                id,
                family: actual.family.into(),
                weight: actual.weight,
                style: actual.style,
                derivation_id: binding_id.clone(),
                content_hash: actual.content_hash.into(),
                size_bytes: actual.size_bytes.to_string(),
            },
            source_commit: actual.source_commit.into(),
            source_url: actual.source_url.into(),
            source_sha256: actual.content_hash.into(),
            source_size_bytes: actual.size_bytes.to_string(),
            license_url: actual.license_url.into(),
            license_id: actual.license_identifier.into(),
            license_notice: actual.license_notice.into(),
            axes: Vec::new(),
            proof_json: canonical(&proof)?,
        },
        binding: FontSynthesisBinding {
            schema_version: 1,
            binding_id,
            profile,
            actual: entry,
            requested,
        },
    })
}
pub fn catalog() -> Result<Vec<FontSynthesisSource>, EditorError> {
    Ok(catalog::records()?
        .iter()
        .map(|record| record.source.clone())
        .collect())
}
pub(crate) fn contains_source(id: &str) -> Result<bool, EditorError> {
    Ok(catalog::records()?
        .iter()
        .any(|record| record.source.source.face.id == id))
}
pub fn source(id: &str) -> Result<FontSynthesisSource, EditorError> {
    Ok(catalog::by_id(id)?.source.clone())
}
pub fn pinned_source(id: &str) -> Result<PinnedDerivedFont, EditorError> {
    Ok(PinnedDerivedFont {
        source: source(id)?.source,
    })
}
pub fn validate_pinned_source(pin: &PinnedDerivedFont) -> Result<(), EditorError> {
    let expected = source(&pin.source.face.id)?;
    if pin.source != expected.source {
        return Err(invalid(
            "Font synthesis proof differs from the exact compiled source/request/license",
        ));
    }
    Ok(())
}
/// Native acquisition uses only the original compiled source, never an URL or
/// asserted native identity taken from the opaque request/proof envelope.
pub fn actual_source(pin: &PinnedDerivedFont) -> Result<&'static PinnedFontSource, EditorError> {
    validate_pinned_source(pin)?;
    match &catalog::by_id(&pin.source.face.id)?.base {
        SynthesisBaseSource::RepositoryStatic(actual) => Ok(*actual),
        SynthesisBaseSource::ApprovedStatic(actual) => Ok(&actual.source),
        _ => Err(invalid(
            "Derived synthesis bases require their original output acquisition route",
        )),
    }
}
fn resource(id: &str, pin: &FontSynthesisSource) -> Result<FontResource, EditorError> {
    let record = catalog::by_id(&pin.source.face.id)?;
    let actual = &record.source.binding.actual;
    let size = actual
        .size_bytes
        .parse()
        .map_err(|_| invalid("Base output size is invalid"))?;
    Ok(FontResource {
        id: id.into(),
        family: actual.family.clone(),
        weight: actual.weight,
        style: actual.style,
        face_index: 0,
        content: AssetVersion {
            id: format!("av_{}", actual.content_hash),
            content_hash: actual.content_hash.clone(),
            size,
        },
        origin: ResourceOrigin::Generated {
            provider: FONT_SYNTHESIS_PROVIDER.into(),
            generation_id: pin.binding.binding_id.clone(),
        },
        license: Some(record.license.clone()),
    })
}
pub fn validate_resource_source(
    font: &FontResource,
    source: &FontDerivedSource,
) -> Result<(), EditorError> {
    validate_pinned_source(&PinnedDerivedFont {
        source: source.clone(),
    })?;
    if !text(&font.id, 1024) || *font != resource(&font.id, &self::source(&source.face.id)?)? {
        return Err(EditorError::new(
            "FONT_SOURCE_INTEGRITY_ERROR",
            "Font resource differs from its actual source and explicit synthesis binding",
        ));
    }
    Ok(())
}
pub fn binding_for_resource(
    font: &FontResource,
) -> Result<Option<FontSynthesisBinding>, EditorError> {
    let ResourceOrigin::Generated {
        provider,
        generation_id,
    } = &font.origin
    else {
        return Ok(None);
    };
    if provider != FONT_SYNTHESIS_PROVIDER {
        return Ok(None);
    }
    let pin = &catalog::by_binding(generation_id)?.source;
    validate_resource_source(font, &pin.source)?;
    Ok(Some(pin.binding.clone()))
}
pub fn source_for_resource(
    font: &FontResource,
) -> Result<Option<FontSynthesisSource>, EditorError> {
    let Some(binding) = binding_for_resource(font)? else {
        return Ok(None);
    };
    Ok(Some(
        catalog::by_binding(&binding.binding_id)?.source.clone(),
    ))
}
pub fn display_family_for_resource(font: &FontResource) -> Result<Option<String>, EditorError> {
    let Some(binding) = binding_for_resource(font)? else {
        return Ok(None);
    };
    Ok(Some(
        catalog::by_binding(&binding.binding_id)?.display.clone(),
    ))
}
/// A sampled graph omits inactive/unreferenced fonts. Legacy public consumers
/// therefore reject on the complete fixed snapshot before compiling it.
pub(crate) fn reject_legacy_project(project: &EditorDocument) -> Result<(), EditorError> {
    if project
        .fonts
        .iter()
        .any(opencut_editor_core::scene::is_font_synthesis_resource)
    {
        return Err(EditorError::new(
            "UNSUPPORTED_V4",
            "Explicit font synthesis requires the versioned Scene or Canvas profile",
        ));
    }
    Ok(())
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
    .any(|value| !text(value, 1024))
    {
        return Err(invalid(
            "Synthesis import requires bounded fixed project/revision/font identities",
        ));
    }
    let pin = source(&request.catalog_id)?;
    validate_font_envelope(
        pin.binding
            .actual
            .size_bytes
            .parse()
            .map_err(|_| invalid("Base output size is invalid"))?,
        &request.required_text,
        &FontImportPolicy::default(),
    )
}
pub fn expected_import_resource(request: &ImportFont) -> Result<FontResource, EditorError> {
    validate_import_request(request)?;
    resource(&request.font_id, &source(&request.catalog_id)?)
}
pub fn prepare_synthesis_font(
    provider: &impl DerivedFontSource,
    inspector: &impl FontInspector,
    request: &ImportFont,
    fetch_policy: &FontFetchPolicy,
    inspection_policy: &FontImportPolicy,
    checkpoint: &mut dyn FontImportCheckpoint,
) -> Result<PreparedDerivedFont, EditorError> {
    checkpoint.check()?;
    let expected = expected_import_resource(request)?;
    let pin = pinned_source(&request.catalog_id)?;
    // Cached bytes do not require network permission; the native acquisition
    // adapter checks that permission only if a public fetch is necessary.
    let limits = acquisition_limits(&request.catalog_id)?;
    if !(1..=limits.max_source_bytes).contains(&fetch_policy.max_bytes)
        || !(1..=limits.max_deadline_ms).contains(&fetch_policy.deadline_ms)
    {
        return Err(invalid(
            "Font synthesis fetch policy exceeds its bounded envelope",
        ));
    }
    if expected.content.size > fetch_policy.max_bytes {
        return Err(EditorError::new(
            "FONT_BUDGET_EXCEEDED",
            "Actual font exceeds the fetch byte budget",
        ));
    }
    validate_font_envelope(
        expected.content.size,
        &request.required_text,
        inspection_policy,
    )?;
    let bytes = provider.obtain(&pin, fetch_policy, checkpoint)?;
    checkpoint.check()?;
    validate_font_input(&bytes, &request.required_text, inspection_policy)?;
    let mut hash = Sha256::new();
    for chunk in bytes.chunks(65_536) {
        checkpoint.check()?;
        hash.update(chunk);
    }
    if bytes.len() as u64 != expected.content.size
        || format!("{:x}", hash.finalize()) != expected.content.content_hash
    {
        return Err(EditorError::new(
            "FONT_SOURCE_INTEGRITY_ERROR",
            "Synthesis must retain the exact unmodified source bytes",
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
            "Synthesis source metadata must remain the actual normal face",
        ));
    }
    if parsed.glyph_count == 0 {
        return Err(EditorError::new(
            "INVALID_FONT",
            "Synthesis source contains no glyphs",
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
