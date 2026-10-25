//! A larger source is allowed only for the two fixed Chiron targets; source
//! acquisition and output inspection budgets remain distinct.
use super::*;
pub(super) const MAX_SOURCE_BYTES: u64 = 64 * 1024 * 1024;
pub(super) fn source_size(value: &str) -> Result<u64, EditorError> {
    let n = value
        .parse::<u64>()
        .map_err(|_| invalid("Invalid source size"))?;
    if n == 0 || n > MAX_SOURCE_BYTES || n.to_string() != value {
        return Err(invalid("Derived source exceeds its fixed source envelope"));
    }
    Ok(n)
}
pub(super) fn validate(source: &FontDerivedSource, recipe: &Value) -> Result<(), EditorError> {
    let size = source_size(&source.source_size_bytes)?;
    if size <= MAX_FONT_BYTES {
        return Ok(());
    }
    static PINS: std::sync::OnceLock<Vec<Value>> = std::sync::OnceLock::new();
    let pins = PINS.get_or_init(|| {
        serde_json::from_str(include_str!("../font_catalog/large_variable_inputs.json"))
            .expect("fixed Chiron source inputs")
    });
    if recipe["recipe_kind"] != "instantiate_renamed"
        || !pins.iter().any(|pin| {
            pin["source_path"] == recipe["source_path"]
                && pin["source_sha256"] == source.source_sha256
                && pin["source_size"].as_u64() == Some(size)
                && pin["source_git_blob"] == recipe["source_git_blob"]
                && pin["weight"].as_u64() == Some(u64::from(source.face.weight))
                && pin["style"] == recipe["style"]
                && pin["axes"] == recipe["axes"]
        })
    {
        return Err(invalid(
            "Large source is not an exact compiled Chiron target",
        ));
    }
    Ok(())
}
