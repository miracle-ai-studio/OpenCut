//! Additive sources and discovery; old catalog/source operations remain frozen.
use crate::font_import::catalog::PinnedFontSource;
use opencut_contracts::{EditorError, font_catalog::FontRenderProfile};

mod catalog;
pub mod distribution;
mod import;
pub use catalog::{catalog, source};
pub use import::{
    expected_import_resource, import_static_font, validate_fetch, validate_import_request,
};

pub const MAX_CATALOG_FONT_BYTES: u64 = 64 * 1024 * 1024;
/// The expanded envelope belongs only to exact approved V3 outputs. Legacy
/// import DTOs/policies and unknown source bytes retain their 32 MiB admission.
pub fn validate_catalog_font_policy(
    policy: &crate::font_import::FontImportPolicy,
) -> Result<(), EditorError> {
    if !(1..=MAX_CATALOG_FONT_BYTES).contains(&policy.max_font_bytes)
        || !(1..=crate::font_import::MAX_FONT_TEXT_BYTES).contains(&policy.max_text_bytes)
    {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Catalog font policy exceeds its bounded envelope",
        ));
    }
    Ok(())
}
pub fn validate_catalog_font_envelope(
    hash: &str,
    size: u64,
    text: &str,
    policy: &crate::font_import::FontImportPolicy,
) -> Result<(), EditorError> {
    use crate::font_import::{FontImportPolicy, MAX_FONT_BYTES, validate_font_envelope};
    validate_catalog_font_policy(policy)?;
    if size == 0 || size > policy.max_font_bytes {
        return Err(EditorError::new(
            "FONT_BUDGET_EXCEEDED",
            "Font byte limit exceeded",
        ));
    }
    if size > MAX_FONT_BYTES && approved_native_source(hash, size).is_none() {
        return Err(EditorError::new(
            "FONT_BUDGET_EXCEEDED",
            "Large font requires an exact approved catalog output",
        ));
    }
    // Reuse unchanged text/control validation without pretending that a large
    // font itself was admitted by the legacy byte envelope.
    validate_font_envelope(
        size.min(MAX_FONT_BYTES),
        text,
        &FontImportPolicy {
            max_font_bytes: policy.max_font_bytes.min(MAX_FONT_BYTES),
            max_text_bytes: policy.max_text_bytes,
        },
    )
}

#[derive(Clone, Copy, Debug)]
pub struct ApprovedFontSource {
    pub source: PinnedFontSource,
    pub render_profile: FontRenderProfile,
}

#[derive(Clone, Copy, Debug)]
pub struct StaticFontEntry {
    pub approved: ApprovedFontSource,
    pub display_family: &'static str,
    pub alias_reason: Option<&'static str>,
}

static STATIC_SOURCES: &[StaticFontEntry] = include!("static_generated.rs");
static COLOR_SOURCES: &[StaticFontEntry] = include!("color_generated.rs");
static SUPPLEMENTAL_SOURCES: &[StaticFontEntry] = include!("supplemental_generated.rs");

pub fn static_sources() -> impl Iterator<Item = &'static StaticFontEntry> {
    STATIC_SOURCES
        .iter()
        .chain(COLOR_SOURCES)
        .chain(SUPPLEMENTAL_SOURCES)
}

pub fn native_source(id: &str) -> Result<&'static ApprovedFontSource, EditorError> {
    static_sources()
        .find(|entry| entry.approved.source.catalog_id == id)
        .map(|entry| &entry.approved)
        .ok_or_else(|| EditorError::new("NOT_FOUND", "Unknown pinned V3 font source"))
}

pub fn approved_native_source(
    content_hash: &str,
    size: u64,
) -> Option<&'static ApprovedFontSource> {
    static_sources()
        .find(|entry| {
            entry.approved.source.content_hash == content_hash
                && entry.approved.source.size_bytes == size
        })
        .map(|entry| &entry.approved)
        .or_else(|| distribution::approved_native_source(content_hash, size))
        .or_else(|| {
            approved_derived_sources().ok()?.iter().find(|entry| {
                entry.source.content_hash == content_hash && entry.source.size_bytes == size
            })
        })
}

// The compiled V2 recipe store has no dependency on this module. Borrow its
// immutable strings; keep only the small parsed profile/license digest table
// here. Runtime requests cannot insert approved records or change an identity.
fn approved_derived_sources() -> Result<&'static [ApprovedFontSource], EditorError> {
    struct Metadata {
        profile: FontRenderProfile,
        license_hash: String,
    }
    static META: std::sync::OnceLock<Result<Vec<Metadata>, EditorError>> =
        std::sync::OnceLock::new();
    static SOURCES: std::sync::OnceLock<Result<Vec<ApprovedFontSource>, EditorError>> =
        std::sync::OnceLock::new();
    let error = || {
        EditorError::new(
            "FONT_SOURCE_INTEGRITY_ERROR",
            "Compiled derived render identity is invalid",
        )
    };
    SOURCES
        .get_or_init(|| {
            let pins: Vec<_> = crate::font_named::compiled_sources()?
                .iter()
                .chain(crate::font_color_static::compiled_sources()?.iter())
                .collect();
            let metadata = META
                .get_or_init(|| {
                    use sha2::{Digest, Sha256};
                    pins.iter()
                        .map(|pin| {
                            let proof: serde_json::Value =
                                serde_json::from_str(&pin.source.proof_json)
                                    .map_err(|_| error())?;
                            let profile =
                                serde_json::from_value(proof["source"]["render_profile"].clone())
                                    .map_err(|_| error())?;
                            Ok(Metadata {
                                profile,
                                license_hash: format!(
                                    "{:x}",
                                    Sha256::digest(pin.source.license_notice.as_bytes())
                                ),
                            })
                        })
                        .collect::<Result<Vec<_>, EditorError>>()
                })
                .as_ref()
                .map_err(Clone::clone)?;
            pins.iter()
                .zip(metadata)
                .map(|(pin, metadata)| {
                    let pin = *pin;
                    let source = &pin.source;
                    let size_bytes = source.face.size_bytes.parse().map_err(|_| error())?;
                    Ok(ApprovedFontSource {
                        render_profile: metadata.profile,
                        source: PinnedFontSource {
                            catalog_id: &source.face.id,
                            family: &source.face.family,
                            weight: source.face.weight,
                            style: source.face.style,
                            source_commit: &source.source_commit,
                            source_url: &source.source_url,
                            content_hash: &source.face.content_hash,
                            size_bytes,
                            license_url: &source.license_url,
                            license_sha256: &metadata.license_hash,
                            license_identifier: &source.license_id,
                            license_notice: &source.license_notice,
                            attribution: "",
                        },
                    })
                })
                .collect::<Result<Vec<_>, EditorError>>()
        })
        .as_ref()
        .map(Vec::as_slice)
        .map_err(Clone::clone)
}

#[cfg(test)]
mod tests;

pub mod selection;
