//! Check every reviewed output identity, separately from the retained legal
//! notice/copyright/trademark strings, which must keep their original bytes.
use super::*;
const IDS: [u64; 12] = [1, 2, 3, 4, 6, 16, 17, 18, 20, 21, 22, 25];
const PROFILE: &str = "opencut-fixed-opaque-names-v1";
const UBUNTU_PROFILE: &str = "opencut-ubuntu-derivative-names-v1";
pub(super) fn validate_names(
    source: &FontDerivedSourceV1,
    proof: &Value,
) -> Result<(), EditorError> {
    let recipe = &proof["source"];
    let display = recipe["source_family"]
        .as_str()
        .ok_or_else(|| invalid("Original display family missing"))?;
    let profile = if source.license_id == "Ubuntu-font-1.0" {
        UBUNTU_PROFILE
    } else {
        PROFILE
    };
    if !text(display, 512)
        || recipe["name_rewrite_version"] != profile
        || recipe["rewritten_name_ids"] != serde_json::json!(IDS)
    {
        return Err(invalid("Unapproved opaque name rewrite profile"));
    }
    let identity = serde_json::json!({"profile":profile,"source_sha256":source.source_sha256,"axes":recipe["axes"],"weight":source.face.weight,"style":recipe["style"]});
    let digest = format!(
        "{:x}",
        Sha256::digest(super::validation::canonical(&identity)?.as_bytes())
    );
    let (family, ps) = if profile == UBUNTU_PROFILE {
        if !matches!(display, "Ubuntu Sans" | "Ubuntu Sans Mono") {
            return Err(invalid("Unreviewed Ubuntu derivative family"));
        }
        (
            format!("{display} derivative OCDF {}", &digest[..24]),
            format!(
                "{}-derivative-OCDF-{}",
                display.replace(' ', ""),
                &digest[..24]
            ),
        )
    } else {
        (
            format!("OCDF {}", &digest[..24]),
            format!("OCDF-{}", &digest[..24]),
        )
    };
    if profile == UBUNTU_PROFILE
        && recipe["derivative_notice"]
            != format!(
                "{family}; Modified Version: fixed static instance and derivative naming; original outlines retained."
            )
    {
        return Err(invalid("Ubuntu modified-version declaration missing"));
    }
    if source.face.family != family || recipe["output_postscript_name"] != ps {
        return Err(invalid(
            "Renamed font is not the deterministic opaque output identity",
        ));
    }
    let reserved = recipe["reserved_names"]
        .as_array()
        .ok_or_else(|| invalid("Reserved names are missing"))?;
    let header = source
        .license_notice
        .split("SIL OPEN FONT LICENSE")
        .next()
        .unwrap_or("")
        .to_lowercase();
    if reserved.len() > 32 || reserved.is_empty() == header.contains("reserved font name") {
        return Err(invalid("Original reserved-name declaration differs"));
    }
    if !matches!(
        recipe["recipe_kind"].as_str(),
        Some("instantiate_renamed" | "static_metadata_corrected" | "static_checksum_repaired")
    ) || !matches!(
        recipe["render_profile"].as_str(),
        Some("static_outline" | "colr_v0" | "colr_v1" | "monochrome_bitmap")
    ) {
        return Err(invalid(
            "Unapproved renamed derivation kind or render profile",
        ));
    }
    super::corrections::validate_static_recipe(source, proof)?;
    let mut previous = None;
    for name in reserved {
        let name = name
            .as_str()
            .ok_or_else(|| invalid("Invalid reserved name"))?;
        if !text(name, 256)
            || previous.is_some_and(|old: &str| old >= name)
            || !header.contains(&name.to_lowercase())
            || family.to_lowercase().contains(&name.to_lowercase())
            || ps.to_lowercase().contains(&name.to_lowercase())
        {
            return Err(invalid(
                "Output font identity retains or misstates an original reserved name",
            ));
        }
        previous = Some(name);
    }
    let rewrite = &proof["name_rewrite"];
    if rewrite["profile"] != profile
        || rewrite["removed_custom_name_ids"] != true
        || !rewrite["cff_identity_rewritten"].is_boolean()
        || !proof["cff2_to_cff"].is_boolean()
    {
        return Err(invalid("Incomplete identity rewrite evidence"));
    }
    let sha = rewrite["original_identity_sha256"].as_str().unwrap_or("");
    if sha.len() != 64
        || !sha
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err(invalid("Original name-table identity hash missing"));
    }
    let removed = rewrite["removed_tables"]
        .as_array()
        .ok_or_else(|| invalid("Removed-table record missing"))?;
    if removed.len() > 2 || removed.iter().any(|v| v != "STAT" && v != "DSIG") {
        return Err(invalid("Unapproved table removal"));
    }
    let style = match (source.face.weight, source.face.style) {
        (400, FontStyleV4::Normal) => "Regular",
        (700, FontStyleV4::Normal) => "Bold",
        (400, FontStyleV4::Italic) => "Italic",
        (700, FontStyleV4::Italic) => "Bold Italic",
        _ => return Err(invalid("Unsupported renamed face")),
    };
    let records = rewrite["output_identity_names"]
        .as_array()
        .ok_or_else(|| invalid("Rewritten name-table manifest missing"))?;
    if records.len() < 24 || records.len() > 4096 {
        return Err(invalid("Name-table manifest exceeds envelope"));
    }
    let mut bindings = std::collections::BTreeSet::new();
    for record in records {
        let id = record["id"]
            .as_u64()
            .ok_or_else(|| invalid("Invalid name ID"))?;
        let coordinate = |key: &str| {
            record[key]
                .as_u64()
                .filter(|v| *v <= 65535)
                .ok_or_else(|| invalid("Invalid name binding"))
        };
        let binding = (
            id,
            coordinate("platform")?,
            coordinate("encoding")?,
            coordinate("language")?,
        );
        let expected = match id {
            1 | 16 | 21 => family.clone(),
            2 | 17 | 22 => style.into(),
            3 => format!("{ps};v2"),
            4 | 18 => format!("{family} {style}"),
            6 | 20 | 25 => ps.clone(),
            _ => return Err(invalid("Unapproved font identity ID")),
        };
        if record["value"] != expected || !bindings.insert(binding) {
            return Err(invalid("Incomplete or conflicting identity rewrite"));
        }
    }
    if IDS
        .iter()
        .any(|id| !bindings.contains(&(*id, 3, 1, 0x409)) || !bindings.contains(&(*id, 0, 4, 0)))
    {
        return Err(invalid("Required Unicode/Windows opaque bindings missing"));
    }
    let geometry = proof["preserved_geometry_tables"]
        .as_object()
        .ok_or_else(|| invalid("Geometry preservation proof missing"))?;
    if geometry.is_empty()
        || geometry.len() > 128
        || geometry.iter().any(|(tag, hash)| {
            tag.len() != 4
                || !hash.as_str().is_some_and(|s| {
                    s.len() == 64
                        && s.bytes()
                            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
                })
        })
    {
        return Err(invalid("Invalid geometry preservation proof"));
    }
    Ok(())
}
