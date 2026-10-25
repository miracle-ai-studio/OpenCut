//! V2 proof validation. V1 validation is deliberately not relaxed.
use super::*;
pub(super) fn invalid(message: &str) -> EditorError {
    EditorError::new("FONT_DERIVATION_INVALID", message)
}
fn hex(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(super) fn size(value: &str) -> Result<u64, EditorError> {
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
pub(super) fn text(value: &str, max: usize) -> bool {
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
pub(crate) fn canonical(value: &Value) -> Result<String, EditorError> {
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
        || !hex(&face.content_hash, 64)
        || !hex(&s.source_sha256, 64)
    {
        return Err(invalid("Derived font pin identity is invalid"));
    }
    let output_size = size(&face.size_bytes)?;
    let source_size = super::large::source_size(&s.source_size_bytes)?;
    if !matches!(
        s.license_id.as_str(),
        "OFL-1.1" | "Apache-2.0" | "Ubuntu-font-1.0"
    ) || s.license_notice.is_empty()
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
        "font-derived-v2_{:x}",
        Sha256::digest(canonical(&proof)?.as_bytes())
    );
    if id != expected || face.derivation_id != id {
        return Err(invalid(
            "Derivation proof id differs from canonical content",
        ));
    }
    if proof["schema_version"] != 2
        || proof["output_sha256"] != face.content_hash
        || proof["output_size"].as_u64() != Some(output_size)
        || proof["license_notice"] != s.license_notice
    {
        return Err(invalid(
            "Derivation proof output/license differs from pinned source",
        ));
    }
    let recipe = &proof["source"];
    super::approved::validate(recipe)?;
    super::large::validate(s, recipe)?;
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
    if !(2..=8).contains(&path.len())
        || path
            .iter()
            .any(|p| p.is_empty() || *p == ".." || p.contains(['\\', ':', '#', '?']))
    {
        return Err(invalid(
            "Derivation source path is not a fixed Google font path",
        ));
    }
    let repository = recipe["source_repository"]
        .as_str()
        .ok_or_else(|| invalid("Fixed source repository missing"))?;
    let license_repository = recipe["license_repository"]
        .as_str()
        .ok_or_else(|| invalid("Fixed license repository missing"))?;
    let license_commit = recipe["license_commit"]
        .as_str()
        .ok_or_else(|| invalid("Fixed license commit missing"))?;
    let license_path = recipe["license_path"]
        .as_str()
        .ok_or_else(|| invalid("Fixed license path missing"))?;
    if !matches!(repository, "google/fonts" | "google/material-design-icons")
        || !matches!(
            license_repository,
            "google/fonts" | "google/material-design-icons"
        )
        || !hex(license_commit, 40)
        || license_path
            .split('/')
            .any(|p| p.is_empty() || p == ".." || p.contains(['\\', ':', '#', '?']))
    {
        return Err(invalid("Unapproved fixed source/license location"));
    }
    let root = format!(
        "https://raw.githubusercontent.com/{repository}/{}/",
        s.source_commit
    );
    let license_root =
        format!("https://raw.githubusercontent.com/{license_repository}/{license_commit}/");
    if s.source_url != format!("{root}{}", source_url_path(source_path))
        || s.license_url != format!("{license_root}{}", source_url_path(license_path))
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
    if !match s.license_id.as_str() {
        "OFL-1.1" => s.license_notice.contains("SIL OPEN FONT LICENSE"),
        "Apache-2.0" => {
            s.license_notice.contains("Apache License") && s.license_notice.contains("Version 2.0")
        }
        "Ubuntu-font-1.0" => s.license_notice.contains("UBUNTU FONT LICENCE Version 1.0"),
        _ => false,
    } {
        return Err(invalid(
            "V2 requires the complete original typed license notice",
        ));
    }
    super::names::validate_names(s, &proof)?;
    let axes = recipe["axes"]
        .as_object()
        .ok_or_else(|| invalid("Derivation recipe axes are missing"))?;
    let static_recipe = matches!(
        recipe["recipe_kind"].as_str(),
        Some("static_metadata_corrected" | "static_checksum_repaired")
    );
    if axes.len() != s.axes.len()
        || (static_recipe && !axes.is_empty())
        || (!static_recipe && axes.is_empty())
    {
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
