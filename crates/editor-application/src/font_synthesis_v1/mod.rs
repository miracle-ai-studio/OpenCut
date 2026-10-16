//! Reviewed normal source plus an explicit, versioned Chromium italic request.
//! This is a rendering binding, not an outline derivation or a new native face.
use crate::{
    font_derived_v1::{DerivedFontSourceV1, PinnedDerivedFontV1, PreparedDerivedFontV1},
    font_import_v1::{
        catalog::{FontFetchPolicyV1, PinnedFontSourceV1, pinned_font_source_v1},
        *,
    },
};
pub use opencut_contracts::font_synthesis_v1::{
    FONT_SYNTHESIS_BINDING_ID_PREFIX_V1, FONT_SYNTHESIS_PROVIDER_V1,
};
use opencut_contracts::{
    AssetVersion, EditorError, font_derived_v1::*, font_import_v1::FontCatalogEntryV1,
    font_synthesis_v1::*, project_v4::*,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

mod catalog;
pub use catalog::{
    SynthesisAcquisitionLimitsV1, SynthesisBaseSourceV1, acquisition_limits, base_priority,
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
fn from_actual(actual: &PinnedFontSourceV1) -> Result<FontSynthesisSourceV1, EditorError> {
    if actual.style != FontStyleV4::Normal
        || !matches!(actual.weight, 400 | 700)
        || actual.size_bytes == 0
        || actual.size_bytes > MAX_FONT_BYTES_V1
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
    let entry = FontCatalogEntryV1 {
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
    let requested = FontSynthesisRequestV1 {
        weight: actual.weight,
        style: FontStyleV4::Italic,
    };
    let profile = FontSynthesisProfileV1::ClassicChromiumItalicV1;
    let mut proof = json!({
        "schema_version": 3,
        "kind": "browser_font_synthesis",
        "provider": FONT_SYNTHESIS_PROVIDER_V1,
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
        "{FONT_SYNTHESIS_BINDING_ID_PREFIX_V1}{:x}",
        Sha256::digest(canonical(&proof)?.as_bytes())
    );
    proof["derivation_id"] = Value::String(binding_id.clone());
    Ok(FontSynthesisSourceV1 {
        source: FontDerivedSourceV1 {
            face: FontDerivedFaceV1 {
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
        binding: FontSynthesisBindingV1 {
            schema_version: 1,
            binding_id,
            profile,
            actual: entry,
            requested,
        },
    })
}
pub fn catalog() -> Result<Vec<FontSynthesisSourceV1>, EditorError> {
    Ok(catalog::records()?
        .iter()
        .map(|record| record.source.clone())
        .collect())
}
pub fn source(id: &str) -> Result<FontSynthesisSourceV1, EditorError> {
    Ok(catalog::by_id(id)?.source.clone())
}
pub fn pinned_source(id: &str) -> Result<PinnedDerivedFontV1, EditorError> {
    Ok(PinnedDerivedFontV1 {
        source: source(id)?.source,
    })
}
pub fn validate_pinned_source_v1(pin: &PinnedDerivedFontV1) -> Result<(), EditorError> {
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
pub fn actual_source(
    pin: &PinnedDerivedFontV1,
) -> Result<&'static PinnedFontSourceV1, EditorError> {
    validate_pinned_source_v1(pin)?;
    match &catalog::by_id(&pin.source.face.id)?.base {
        SynthesisBaseSourceV1::StaticV1(actual) => Ok(*actual),
        SynthesisBaseSourceV1::StaticV3(actual) => Ok(&actual.source),
        _ => Err(invalid(
            "Derived synthesis bases require their original output acquisition route",
        )),
    }
}
fn resource(id: &str, pin: &FontSynthesisSourceV1) -> Result<FontResourceV4, EditorError> {
    let record = catalog::by_id(&pin.source.face.id)?;
    let actual = &record.source.binding.actual;
    let size = actual
        .size_bytes
        .parse()
        .map_err(|_| invalid("Base output size is invalid"))?;
    Ok(FontResourceV4 {
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
        origin: ResourceOriginV4::Generated {
            provider: FONT_SYNTHESIS_PROVIDER_V1.into(),
            generation_id: pin.binding.binding_id.clone(),
        },
        license: Some(record.license.clone()),
    })
}
pub fn validate_resource_source(
    font: &FontResourceV4,
    source: &FontDerivedSourceV1,
) -> Result<(), EditorError> {
    validate_pinned_source_v1(&PinnedDerivedFontV1 {
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
    font: &FontResourceV4,
) -> Result<Option<FontSynthesisBindingV1>, EditorError> {
    let ResourceOriginV4::Generated {
        provider,
        generation_id,
    } = &font.origin
    else {
        return Ok(None);
    };
    if provider != FONT_SYNTHESIS_PROVIDER_V1 {
        return Ok(None);
    }
    let pin = &catalog::by_binding(generation_id)?.source;
    validate_resource_source(font, &pin.source)?;
    Ok(Some(pin.binding.clone()))
}
pub fn source_for_resource(
    font: &FontResourceV4,
) -> Result<Option<FontSynthesisSourceV1>, EditorError> {
    let Some(binding) = binding_for_resource(font)? else {
        return Ok(None);
    };
    Ok(Some(
        catalog::by_binding(&binding.binding_id)?.source.clone(),
    ))
}
pub fn display_family_for_resource(font: &FontResourceV4) -> Result<Option<String>, EditorError> {
    let Some(binding) = binding_for_resource(font)? else {
        return Ok(None);
    };
    Ok(Some(
        catalog::by_binding(&binding.binding_id)?.display.clone(),
    ))
}
/// A sampled graph omits inactive/unreferenced fonts. Legacy public consumers
/// therefore reject on the complete fixed snapshot before compiling it.
pub(crate) fn reject_legacy_project(project: &ProjectDocumentV4) -> Result<(), EditorError> {
    if project
        .fonts
        .iter()
        .any(opencut_editor_core::project_v4::is_font_synthesis_resource_v1)
    {
        return Err(EditorError::new(
            "UNSUPPORTED_V4",
            "Explicit font synthesis requires the versioned Scene or Canvas profile",
        ));
    }
    Ok(())
}
pub fn validate_import_request(request: &ImportDerivedFontV1) -> Result<(), EditorError> {
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
    validate_font_envelope_v1(
        pin.binding
            .actual
            .size_bytes
            .parse()
            .map_err(|_| invalid("Base output size is invalid"))?,
        &request.required_text,
        &FontImportPolicyV1::default(),
    )
}
pub fn expected_import_resource(
    request: &ImportDerivedFontV1,
) -> Result<FontResourceV4, EditorError> {
    validate_import_request(request)?;
    resource(&request.font_id, &source(&request.catalog_id)?)
}
pub fn prepare_synthesis_font(
    provider: &impl DerivedFontSourceV1,
    inspector: &impl FontInspectorV1,
    request: &ImportDerivedFontV1,
    fetch_policy: &FontFetchPolicyV1,
    inspection_policy: &FontImportPolicyV1,
    checkpoint: &mut dyn FontImportCheckpointV1,
) -> Result<PreparedDerivedFontV1, EditorError> {
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
    validate_font_envelope_v1(
        expected.content.size,
        &request.required_text,
        inspection_policy,
    )?;
    let bytes = provider.obtain(&pin, fetch_policy, checkpoint)?;
    checkpoint.check()?;
    validate_font_input_v1(&bytes, &request.required_text, inspection_policy)?;
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
    Ok(PreparedDerivedFontV1 {
        font: PreparedFontV1 {
            resource: expected,
            glyph_count: parsed.glyph_count,
            checked_codepoints: parsed.checked_codepoints,
        },
        bytes,
    })
}
