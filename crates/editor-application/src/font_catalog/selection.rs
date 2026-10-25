//! Select an admitted acquisition algorithm by exact compiled source identity.
//! A caller cannot choose a protocol, algorithm, runtime, URL or proof variant.
use super::*;
use opencut_contracts::font_import::ImportFont;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontAcquisition {
    RepositoryStatic,
    ApprovedStatic,
    Outline,
    RenamedOutline,
    CompleteColor,
    Synthesis,
    Distribution,
}
impl FontAcquisition {
    pub fn derived(self) -> bool {
        matches!(
            self,
            Self::Outline | Self::RenamedOutline | Self::CompleteColor | Self::Synthesis
        )
    }
}
pub fn select(id: &str) -> Result<FontAcquisition, EditorError> {
    if id.trim().is_empty() || id.len() > 1024 || id.chars().any(char::is_control) {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Font source identity is invalid",
        ));
    }
    // The full catalog validates identity uniqueness. Exact membership checks
    // stay lazy so a static/distribution source does not initialize every
    // derivation or synthesis proof store merely to select its own algorithm.
    if crate::font_import::catalog::static_font_inventory()
        .iter()
        .any(|pin| pin.catalog_id == id)
    {
        return Ok(FontAcquisition::RepositoryStatic);
    }
    if static_sources().any(|pin| pin.approved.source.catalog_id == id) {
        return Ok(FontAcquisition::ApprovedStatic);
    }
    if distribution::contains_source(id) {
        return Ok(FontAcquisition::Distribution);
    }
    if crate::font_derived::contains_source(id)? {
        return Ok(FontAcquisition::Outline);
    }
    if crate::font_named::contains_source(id)? {
        return Ok(FontAcquisition::RenamedOutline);
    }
    if crate::font_color_static::compiled_sources()?
        .iter()
        .any(|pin| pin.source.face.id == id)
    {
        return Ok(FontAcquisition::CompleteColor);
    }
    if crate::font_synthesis::contains_source(id)? {
        return Ok(FontAcquisition::Synthesis);
    }
    Err(EditorError::new("NOT_FOUND", "Unknown pinned font source"))
}

pub fn validate(request: &ImportFont) -> Result<FontAcquisition, EditorError> {
    let kind = select(&request.catalog_id)?;
    match kind {
        FontAcquisition::RepositoryStatic => crate::font_import::validate_import_request(request)?,
        FontAcquisition::ApprovedStatic => validate_import_request(request)?,
        FontAcquisition::Distribution => distribution::validate_request(request)?,
        _ => crate::font_derived::provenance::validate_import_request(request)?,
    }
    Ok(kind)
}
