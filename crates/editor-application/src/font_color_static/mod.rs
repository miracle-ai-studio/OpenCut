//! Complete static COLRv1 successors; frozen V1/V2 recipes remain separate.
use crate::{
    font_derived::{DerivedFontSource, PinnedDerivedFont, PreparedDerivedFont},
    font_import::{catalog::FontFetchPolicy, *},
};
use opencut_contracts::font_catalog::{FontFace, FontKind, FontRenderProfile};
use opencut_contracts::{AssetVersion, EditorError, font_derived::*, scene::*};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub const PROVIDER: &str = "opencut-fonttools-static-colr-v1";
const PROFILE: &str = "opencut-static-colr-opaque-names-v1";
fn invalid(message: &str) -> EditorError {
    EditorError::new("FONT_DERIVATION_INVALID", message)
}
fn canonical(value: &Value) -> Result<String, EditorError> {
    crate::font_named::canonical(value)
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn text(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 1024 && !value.chars().any(char::is_control)
}
fn size(value: &str) -> Result<u64, EditorError> {
    let n: u64 = value
        .parse()
        .map_err(|_| invalid("Invalid static color size"))?;
    if n == 0 || n > MAX_FONT_BYTES || n.to_string() != value {
        return Err(invalid(
            "Static color size exceeds canonical bounded envelope",
        ));
    }
    Ok(n)
}

fn validate_record(source: &FontDerivedSource) -> Result<(), EditorError> {
    let mut proof: Value = serde_json::from_str(&source.proof_json)
        .map_err(|_| invalid("Invalid static color proof"))?;
    if source.proof_json.len() > 262144 || canonical(&proof)? != source.proof_json {
        return Err(invalid("Static color proof is not bounded canonical JSON"));
    }
    let base: FontDerivedSource = serde_json::from_value(proof["base_source"].clone())
        .map_err(|_| invalid("Frozen V2 base source missing"))?;
    let original = crate::font_named::source(&base.face.id)?.source;
    if original != base {
        return Err(invalid(
            "Static color base differs from exact frozen V2 source",
        ));
    }
    let tool: Value = serde_json::from_str(include_str!(
        "../../../../scripts/font-catalog/color-static-runtime-lock.json"
    ))
    .map_err(|_| invalid("Compiled static color runtime lock invalid"))?;
    let id = proof
        .as_object_mut()
        .ok_or_else(|| invalid("Static color proof object missing"))?
        .remove("derivation_id")
        .ok_or_else(|| invalid("Static color proof id missing"))?;
    let expected_id = format!(
        "font-color-static-v1_{}",
        digest(canonical(&proof)?.as_bytes())
    );
    let names = json!({"profile":PROFILE,"base_derivation_id":base.face.derivation_id,
        "color_algorithm_sha256":tool["derivation_sha256"]});
    let name_hash = digest(canonical(&names)?.as_bytes());
    let expected_family = format!("OCDF COLR {}", &name_hash[..24]);
    let expected_source_id = format!(
        "{}-color-static-v1",
        base.face
            .id
            .strip_suffix("-derived-v2")
            .ok_or_else(|| invalid("Static color base id is not versioned V2"))?
    );
    let base_proof: Value =
        serde_json::from_str(&base.proof_json).map_err(|_| invalid("Frozen V2 proof invalid"))?;
    let mut recipe = base_proof["source"].clone();
    recipe["family"] = json!(expected_family);
    recipe["output_postscript_name"] = json!(format!("OCDF-COLR-{}", &name_hash[..24]));
    recipe["name_rewrite_version"] = json!(PROFILE);
    recipe["recipe_kind"] = json!("instantiate_static_colr_v1");
    let color = &proof["color_instantiation"];
    if id != expected_id
        || source.face.derivation_id != expected_id
        || source.face.id != expected_source_id
        || source.face.family != expected_family
        || source.face.weight != base.face.weight
        || source.face.style != base.face.style
        || proof["schema_version"] != 4
        || proof["kind"] != "complete_static_colr"
        || proof["source"] != recipe
        || proof["tool"] != tool
        || tool["base_derivation_sha256"] != base_proof["tool"]["derivation_sha256"]
        || proof["output_sha256"] != source.face.content_hash
        || source.face.content_hash.len() != 64
        || !source
            .face
            .content_hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || proof["output_size"].as_u64() != Some(size(&source.face.size_bytes)?)
        || proof["license_notice"] != source.license_notice
        || color["profile"] != "opencut-colr-complete-static-v1"
        || color["removed_var_store"] != true
        || color["removed_var_index_map"] != true
        || !color["variable_tables"].as_u64().is_some_and(|n| n > 0)
        || !color["variable_fields"].as_u64().is_some_and(|n| n > 0)
        || source.source_commit != base.source_commit
        || source.source_url != base.source_url
        || source.source_sha256 != base.source_sha256
        || source.source_size_bytes != base.source_size_bytes
        || source.license_url != base.license_url
        || source.license_id != base.license_id
        || source.license_notice != base.license_notice
        || source.axes != base.axes
    {
        return Err(invalid(
            "Static color proof differs from compiled source/axes/license/algorithm/output",
        ));
    }
    Ok(())
}

pub fn compiled_sources() -> Result<&'static [PinnedDerivedFont], EditorError> {
    static SOURCES: std::sync::OnceLock<Result<Vec<PinnedDerivedFont>, EditorError>> =
        std::sync::OnceLock::new();
    SOURCES
        .get_or_init(|| {
            let records: Vec<FontDerivedSource> =
                serde_json::from_str(include_str!("catalog_pins.json"))
                    .map_err(|_| invalid("Compiled static color source catalog invalid"))?;
            if records.len() != 17 {
                return Err(invalid("Static color source set differs"));
            }
            let mut ids = std::collections::BTreeSet::new();
            records
                .into_iter()
                .map(|source| {
                    validate_record(&source)?;
                    if !ids.insert(source.face.id.clone()) {
                        return Err(invalid("Duplicate static color source"));
                    }
                    Ok(PinnedDerivedFont { source })
                })
                .collect()
        })
        .as_ref()
        .map(Vec::as_slice)
        .map_err(Clone::clone)
}
pub fn source(id: &str) -> Result<PinnedDerivedFont, EditorError> {
    compiled_sources()?
        .iter()
        .find(|pin| pin.source.face.id == id)
        .cloned()
        .ok_or_else(|| EditorError::new("NOT_FOUND", "Unknown compiled static color font source"))
}
pub fn validate_pinned_source(pin: &PinnedDerivedFont) -> Result<(), EditorError> {
    if source(&pin.source.face.id)?.source != pin.source {
        return Err(invalid(
            "Static color source differs from exact compiled proof",
        ));
    }
    Ok(())
}
pub fn replaces(id: &str) -> bool {
    id.strip_suffix("-derived-v2")
        .is_some_and(|stem| source(&format!("{stem}-color-static-v1")).is_ok())
}
pub fn display_family(id: &str) -> Result<String, EditorError> {
    let pin = source(id)?;
    let proof: Value = serde_json::from_str(&pin.source.proof_json)
        .map_err(|_| invalid("Invalid static color display proof"))?;
    proof["source"]["source_family"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| invalid("Static color display family missing"))
}
pub fn face(source: &FontDerivedSource) -> Result<FontFace, EditorError> {
    Ok(FontFace {
        id: source.face.id.clone(), display_family: display_family(&source.face.id)?,
        family: source.face.family.clone(), weight: source.face.weight, style: source.face.style,
        requested_style: None, kind: FontKind::Derived,
        content_hash: source.face.content_hash.clone(), size_bytes: source.face.size_bytes.clone(),
        render_profile: FontRenderProfile::ColrV1,
        alias_reason: Some("Complete static color instance with its licensed derivative name and retained original source proof.".into()),
        import_available: false, unavailable_reason: Some("Host availability has not been attached.".into()),
    })
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
    .any(|v| !text(v))
    {
        return Err(invalid("Invalid static color request identities"));
    }
    let pin = source(&request.catalog_id)?;
    validate_font_envelope(
        size(&pin.source.face.size_bytes)?,
        &request.required_text,
        &FontImportPolicy::default(),
    )
}
fn resource(id: &str, source: &FontDerivedSource) -> Result<FontResource, EditorError> {
    Ok(FontResource {
        id: id.into(),
        family: source.face.family.clone(),
        weight: source.face.weight,
        style: source.face.style,
        face_index: 0,
        content: AssetVersion {
            id: format!("av_{}", source.face.content_hash),
            content_hash: source.face.content_hash.clone(),
            size: size(&source.face.size_bytes)?,
        },
        origin: ResourceOrigin::Generated {
            provider: PROVIDER.into(),
            generation_id: source.face.derivation_id.clone(),
        },
        license: Some(ResourceLicense {
            identifier: Some(source.license_id.clone()),
            source_url: Some(source.license_url.clone()),
            notice: Some(source.license_notice.clone()),
            attribution: None,
        }),
    })
}
pub fn expected_import_resource(request: &ImportFont) -> Result<FontResource, EditorError> {
    validate_import_request(request)?;
    resource(&request.font_id, &source(&request.catalog_id)?.source)
}
pub fn prepare(
    provider: &impl DerivedFontSource,
    inspector: &impl FontInspector,
    request: &ImportFont,
    fetch: &FontFetchPolicy,
    inspection: &FontImportPolicy,
    checkpoint: &mut dyn FontImportCheckpoint,
) -> Result<PreparedDerivedFont, EditorError> {
    checkpoint.check()?;
    let expected = expected_import_resource(request)?;
    let pin = source(&request.catalog_id)?;
    crate::font_named::prepare_pinned_font(
        provider, inspector, request, &pin, expected, fetch, inspection, checkpoint,
    )
}
pub fn validate_resource_source(
    font: &FontResource,
    descriptor: &FontDerivedSource,
) -> Result<(), EditorError> {
    validate_pinned_source(&PinnedDerivedFont {
        source: descriptor.clone(),
    })?;
    if *font != resource(&font.id, descriptor)? {
        return Err(EditorError::new(
            "FONT_SOURCE_INTEGRITY_ERROR",
            "Static color resource differs from retained source proof",
        ));
    }
    Ok(())
}
