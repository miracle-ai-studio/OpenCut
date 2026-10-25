//! A separate immutable input allowlist keeps pin initialization pure. Never
//! query CatalogV3/compiled_sources from this OnceLock initialization path.
use super::*;
pub(super) fn validate(recipe: &Value) -> Result<(), EditorError> {
    static RECIPES: std::sync::OnceLock<Vec<Value>> = std::sync::OnceLock::new();
    let recipes = RECIPES.get_or_init(|| {
        serde_json::from_str(include_str!("approved_recipes.json"))
            .expect("compiled V2 input recipes")
    });
    if !recipes.iter().any(|r| r == recipe) {
        return Err(invalid(
            "Derivation input is not an exact approved fixed repository/commit/bytes/target recipe",
        ));
    }
    Ok(())
}
