//! Reviewed derivation recipes are separate from frozen online font imports.
use crate::font_import::catalog::FontFetchPolicy;
use crate::font_import::*;
use opencut_contracts::{AssetVersion, EditorError, font_derived::*, scene::*};
use serde_json::Value;
use sha2::{Digest, Sha256};
pub mod proof;
pub mod provenance;
#[cfg(test)]
mod tests;
pub const OUTLINE_FONT_PROVIDER: &str = "opencut-fonttools-static-v1";
#[derive(Clone, Debug)]
pub struct PinnedDerivedFont {
    pub source: FontDerivedSource,
}
pub trait DerivedFontSource {
    fn obtain(
        &self,
        source: &PinnedDerivedFont,
        fetch_policy: &FontFetchPolicy,
        checkpoint: &mut dyn FontImportCheckpoint,
    ) -> Result<Vec<u8>, EditorError>;
}
pub struct PreparedDerivedFont {
    pub font: PreparedFont,
    pub bytes: Vec<u8>,
}
fn invalid(message: &str) -> EditorError {
    EditorError::new("FONT_DERIVATION_INVALID", message)
}
fn hex(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn size(value: &str) -> Result<u64, EditorError> {
    let n = value
        .parse::<u64>()
        .map_err(|_| invalid("Invalid derived font size"))?;
    if n == 0 || n > MAX_FONT_BYTES || n.to_string() != value {
        return Err(invalid(
            "Derived font size exceeds canonical bounded envelope",
        ));
    }
    Ok(n)
}
fn text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}
fn source_url_path(path: &str) -> String {
    use std::fmt::Write;
    let mut encoded = String::with_capacity(path.len());
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~".contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            write!(&mut encoded, "%{byte:02X}").expect("writing to a String cannot fail");
        }
    }
    encoded
}
fn canonical(value: &Value) -> Result<String, EditorError> {
    fn sorted(value: &Value) -> Value {
        match value {
            Value::Object(map) => {
                let mut entries = map.iter().collect::<Vec<_>>();
                entries.sort_by(|a, b| a.0.cmp(b.0));
                Value::Object(
                    entries
                        .into_iter()
                        .map(|(k, v)| (k.clone(), sorted(v)))
                        .collect(),
                )
            }
            Value::Array(values) => Value::Array(values.iter().map(sorted).collect()),
            _ => value.clone(),
        }
    }
    serde_json::to_string(&sorted(value)).map_err(|_| invalid("Invalid canonical derivation proof"))
}
pub fn validate_pinned_source(pin: &PinnedDerivedFont) -> Result<(), EditorError> {
    let s = &pin.source;
    let face = &s.face;
    if !text(&face.id, 1024)
        || !text(&face.family, 512)
        || !matches!(face.weight, 400 | 700)
        || !hex(&s.source_commit, 40)
        || s.source_commit != catalog::GOOGLE_FONTS_COMMIT
        || !hex(&face.content_hash, 64)
        || !hex(&s.source_sha256, 64)
    {
        return Err(invalid("Derived font pin identity is invalid"));
    }
    let output_size = size(&face.size_bytes)?;
    let source_size = size(&s.source_size_bytes)?;
    if !matches!(s.license_id.as_str(), "OFL-1.1" | "Apache-2.0")
        || s.license_notice.is_empty()
        || s.license_notice.len() > 65536
        || s.license_notice.contains('\0')
        || s.proof_json.len() > 262144
    {
        return Err(invalid("Derived font license/proof envelope is invalid"));
    }
    let mut proof: Value = serde_json::from_str(&s.proof_json)
        .map_err(|_| invalid("Invalid derivation proof JSON"))?;
    if canonical(&proof)? != s.proof_json {
        return Err(invalid("Derivation proof is not exact canonical JSON"));
    }
    let obj = proof
        .as_object_mut()
        .ok_or_else(|| invalid("Derivation proof must be an object"))?;
    let id = obj
        .remove("derivation_id")
        .and_then(|v| v.as_str().map(str::to_owned))
        .ok_or_else(|| invalid("Derivation proof id is missing"))?;
    let expected = format!(
        "font-derived_{:x}",
        Sha256::digest(canonical(&proof)?.as_bytes())
    );
    if id != expected || face.derivation_id != id {
        return Err(invalid(
            "Derivation proof id differs from canonical content",
        ));
    }
    if proof["schema_version"] != 1
        || proof["output_sha256"] != face.content_hash
        || proof["output_size"].as_u64() != Some(output_size)
        || proof["license_notice"] != s.license_notice
    {
        return Err(invalid(
            "Derivation proof output/license differs from pinned source",
        ));
    }
    let recipe = &proof["source"];
    if recipe["family"] != face.family
        || recipe["weight"].as_u64() != Some(u64::from(face.weight))
        || recipe["style"]
            != match face.style {
                FontStyle::Normal => "normal",
                FontStyle::Italic => "italic",
            }
        || recipe["source_commit"] != s.source_commit
        || recipe["source_sha256"] != s.source_sha256
        || recipe["source_size"].as_u64() != Some(source_size)
        || recipe["license_identifier"] != s.license_id
    {
        return Err(invalid("Derivation source recipe differs from public pin"));
    }
    let source_path = recipe["source_path"]
        .as_str()
        .ok_or_else(|| invalid("Derivation source path is missing"))?;
    let path = source_path.split('/').collect::<Vec<_>>();
    if path.len() != 3
        || !matches!(path[0], "ofl" | "apache")
        || path
            .iter()
            .any(|p| p.is_empty() || *p == ".." || p.contains(['\\', ':', '#', '?']))
    {
        return Err(invalid(
            "Derivation source path is not a fixed Google font path",
        ));
    }
    let root = format!(
        "https://raw.githubusercontent.com/google/fonts/{}/",
        s.source_commit
    );
    let license_path = format!(
        "{}/{}/{}",
        path[0],
        path[1],
        if path[0] == "ofl" {
            "OFL.txt"
        } else {
            "LICENSE.txt"
        }
    );
    if s.source_url != format!("{root}{}", source_url_path(source_path))
        || s.license_url != format!("{root}{}", source_url_path(&license_path))
        || recipe["license_path"] != license_path
        || recipe["license_sha256"] != format!("{:x}", Sha256::digest(s.license_notice.as_bytes()))
        || !recipe["source_git_blob"]
            .as_str()
            .is_some_and(|v| hex(v, 40))
        || !recipe["license_git_blob"]
            .as_str()
            .is_some_and(|v| hex(v, 40))
    {
        return Err(invalid("Derivation source/license provenance differs"));
    }
    if s.license_id == "OFL-1.1" {
        let header = s
            .license_notice
            .split("SIL OPEN FONT LICENSE")
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        if path[0] != "ofl"
            || !s.license_notice.contains("SIL OPEN FONT LICENSE")
            || header.contains("reserved font name")
        {
            return Err(invalid(
                "Derived OFL font requires an original notice without unsupported reserved names",
            ));
        }
    } else if path[0] != "apache"
        || !s.license_notice.contains("Apache License")
        || !s.license_notice.contains("Version 2.0")
    {
        return Err(invalid("Derived Apache font license notice differs"));
    }
    let axes = recipe["axes"]
        .as_object()
        .ok_or_else(|| invalid("Derivation recipe axes are missing"))?;
    if axes.is_empty() || axes.len() != s.axes.len() {
        return Err(invalid("Every derived font axis must be pinned"));
    }
    let mut tags = std::collections::BTreeSet::new();
    for axis in &s.axes {
        if axis.tag.len() != 4
            || !axis.tag.bytes().all(|b| b.is_ascii_alphanumeric())
            || !tags.insert(&axis.tag)
        {
            return Err(invalid("Invalid duplicate derived axis"));
        }
        let coordinate = axes
            .get(&axis.tag)
            .ok_or_else(|| invalid("Derived axis is missing from proof"))?;
        if !coordinate.is_number()
            || coordinate.to_string() != axis.coordinate
            || !coordinate.as_f64().is_some_and(f64::is_finite)
        {
            return Err(invalid(
                "Derived axis coordinate differs from canonical numeric proof",
            ));
        }
    }
    let tool = proof["tool"]
        .as_object()
        .ok_or_else(|| invalid("Derivation tool identity is missing"))?;
    for field in [
        "fonttools_wheel_sha256",
        "fonttools_implementation_sha256",
        "derivation_sha256",
        "static_validator_sha256",
    ] {
        if !tool
            .get(field)
            .and_then(Value::as_str)
            .is_some_and(|v| hex(v, 64))
        {
            return Err(invalid("Derivation implementation hash is missing"));
        }
    }
    for field in ["fonttools_version", "python_version"] {
        if !tool
            .get(field)
            .and_then(Value::as_str)
            .is_some_and(|v| text(v, 128))
        {
            return Err(invalid("Derivation runtime version is missing"));
        }
    }
    Ok(())
}
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
            provider: OUTLINE_FONT_PROVIDER.into(),
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
    if fetch_policy.max_bytes == 0
        || fetch_policy.max_bytes > MAX_FONT_BYTES
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
    let bytes = provider.obtain(&pin, fetch_policy, checkpoint)?;
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
