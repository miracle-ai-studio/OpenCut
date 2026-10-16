//! Only the eighteen reviewed static-source corrections can use these recipe
//! kinds. This is not a permissive repair mode for arbitrary uploaded fonts.
use super::*;
fn hash(value: &Value) -> bool {
    value.as_str().is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
pub(super) fn validate_static_recipe(
    source: &FontDerivedSourceV1,
    proof: &Value,
) -> Result<(), EditorError> {
    let recipe = &proof["source"];
    let kind = recipe["recipe_kind"].as_str().unwrap_or("");
    if kind == "instantiate_renamed" {
        if proof.get("static_correction").is_some() {
            return Err(invalid(
                "Variable recipe cannot claim a static metadata correction",
            ));
        }
        return Ok(());
    }
    if !matches!(
        kind,
        "static_metadata_corrected" | "static_checksum_repaired"
    ) || recipe["render_profile"] != "static_outline"
    {
        return Err(invalid("Unsupported static correction recipe"));
    }
    static REVIEWED: std::sync::OnceLock<Vec<Value>> = std::sync::OnceLock::new();
    let reviewed = REVIEWED.get_or_init(|| {
        serde_json::from_str(include_str!("static_correction_inputs.json"))
            .expect("reviewed static correction inputs")
    });
    let matched = reviewed
        .iter()
        .find(|r| {
            r["source_path"] == recipe["source_path"] && r["source_sha256"] == source.source_sha256
        })
        .ok_or_else(|| {
            invalid("Static repair source is not in the reviewed eighteen-input inventory")
        })?;
    for field in [
        "source_path",
        "source_sha256",
        "source_size",
        "source_git_blob",
        "weight",
        "style",
        "recipe_kind",
        "render_profile",
        "original_metadata",
    ] {
        if recipe[field] != matched[field] {
            return Err(invalid(
                "Static correction differs from reviewed input/target metadata",
            ));
        }
    }
    if recipe["source_family"] != matched["family"] || !source.axes.is_empty() {
        return Err(invalid(
            "Static correction source family or empty-axis identity differs",
        ));
    }
    let correction = &proof["static_correction"];
    let mut allowed = vec!["head", "name", "STAT", "DSIG"];
    if kind == "static_metadata_corrected" {
        allowed.extend(["OS/2", "post"]);
    }
    if correction["original_metadata"] != matched["original_metadata"]
        || correction["target_weight"] != matched["weight"]
        || correction["target_style"] != matched["style"]
        || correction["allowed_changed_tables"] != serde_json::json!(allowed)
    {
        return Err(invalid("Static correction metadata proof differs"));
    }
    let preserved = correction["preserved_source_tables"]
        .as_object()
        .ok_or_else(|| invalid("Source table preservation evidence missing"))?;
    if preserved.len() > 128
        || ["glyf", "hmtx", "cmap"]
            .iter()
            .any(|t| !preserved.contains_key(*t))
        || preserved
            .iter()
            .any(|(tag, sha)| tag.len() != 4 || allowed.contains(&tag.as_str()) || !hash(sha))
    {
        return Err(invalid(
            "Static correction lacks exact outline/metrics/cmap preservation",
        ));
    }
    let changes = correction["changed_tables"]
        .as_object()
        .ok_or_else(|| invalid("Changed static tables missing"))?;
    if changes.is_empty() || changes.len() > allowed.len() {
        return Err(invalid("Static correction changed unexpected table count"));
    }
    for (tag, change) in changes {
        if !allowed.contains(&tag.as_str())
            || !hash(&change["before"])
            || (!hash(&change["after"]) && !matches!(tag.as_str(), "STAT" | "DSIG"))
        {
            return Err(invalid("Static correction changed an unapproved table"));
        }
    }
    Ok(())
}
