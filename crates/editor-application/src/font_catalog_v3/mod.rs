//! Additive sources and discovery; old catalog/source operations remain frozen.
use crate::font_import_v1::catalog::PinnedFontSourceV1;
use opencut_contracts::{EditorError, font_catalog_v3::FontRenderProfileV3};

mod catalog;
mod import;
pub use catalog::{catalog, source};
pub use import::{
    expected_import_resource_v3, import_static_font_v3, validate_fetch_v3,
    validate_import_request_v3,
};

pub const MAX_CATALOG_FONT_BYTES_V3: u64 = 64 * 1024 * 1024;
/// The expanded envelope belongs only to exact approved V3 outputs. Legacy
/// import DTOs/policies and unknown source bytes retain their 32 MiB admission.
pub fn validate_catalog_font_policy_v3(
    policy: &crate::font_import_v1::FontImportPolicyV1,
) -> Result<(), EditorError> {
    if !(1..=MAX_CATALOG_FONT_BYTES_V3).contains(&policy.max_font_bytes)
        || !(1..=crate::font_import_v1::MAX_FONT_TEXT_BYTES_V1).contains(&policy.max_text_bytes)
    {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Catalog font policy exceeds its bounded envelope",
        ));
    }
    Ok(())
}
pub fn validate_catalog_font_envelope_v3(
    hash: &str,
    size: u64,
    text: &str,
    policy: &crate::font_import_v1::FontImportPolicyV1,
) -> Result<(), EditorError> {
    use crate::font_import_v1::{FontImportPolicyV1, MAX_FONT_BYTES_V1, validate_font_envelope_v1};
    validate_catalog_font_policy_v3(policy)?;
    if size == 0 || size > policy.max_font_bytes {
        return Err(EditorError::new(
            "FONT_BUDGET_EXCEEDED",
            "Font byte limit exceeded",
        ));
    }
    if size > MAX_FONT_BYTES_V1 && approved_native_source_v3(hash, size).is_none() {
        return Err(EditorError::new(
            "FONT_BUDGET_EXCEEDED",
            "Large font requires an exact approved catalog output",
        ));
    }
    // Reuse unchanged text/control validation without pretending that a large
    // font itself was admitted by the legacy byte envelope.
    validate_font_envelope_v1(
        size.min(MAX_FONT_BYTES_V1),
        text,
        &FontImportPolicyV1 {
            max_font_bytes: policy.max_font_bytes.min(MAX_FONT_BYTES_V1),
            max_text_bytes: policy.max_text_bytes,
        },
    )
}

#[derive(Clone, Copy, Debug)]
pub struct ApprovedFontSourceV3 {
    pub source: PinnedFontSourceV1,
    pub render_profile: FontRenderProfileV3,
}

#[derive(Clone, Copy, Debug)]
pub struct StaticFontEntryV3 {
    pub approved: ApprovedFontSourceV3,
    pub display_family: &'static str,
    pub alias_reason: Option<&'static str>,
}

static STATIC_SOURCES: &[StaticFontEntryV3] = include!("static_generated.rs");
static COLOR_SOURCES: &[StaticFontEntryV3] = include!("color_generated.rs");
static SUPPLEMENTAL_SOURCES: &[StaticFontEntryV3] = include!("supplemental_generated.rs");

pub fn static_sources_v3() -> impl Iterator<Item = &'static StaticFontEntryV3> {
    STATIC_SOURCES
        .iter()
        .chain(COLOR_SOURCES)
        .chain(SUPPLEMENTAL_SOURCES)
}

pub fn native_source_v3(id: &str) -> Result<&'static ApprovedFontSourceV3, EditorError> {
    static_sources_v3()
        .find(|entry| entry.approved.source.catalog_id == id)
        .map(|entry| &entry.approved)
        .ok_or_else(|| EditorError::new("NOT_FOUND", "Unknown pinned V3 font source"))
}

pub fn approved_native_source_v3(
    content_hash: &str,
    size: u64,
) -> Option<&'static ApprovedFontSourceV3> {
    static_sources_v3()
        .find(|entry| {
            entry.approved.source.content_hash == content_hash
                && entry.approved.source.size_bytes == size
        })
        .map(|entry| &entry.approved)
        .or_else(|| {
            approved_derived_sources().ok()?.iter().find(|entry| {
                entry.source.content_hash == content_hash && entry.source.size_bytes == size
            })
        })
}

// The compiled V2 recipe store has no dependency on this module. Borrow its
// immutable strings; keep only the small parsed profile/license digest table
// here. Runtime requests cannot insert approved records or change an identity.
fn approved_derived_sources() -> Result<&'static [ApprovedFontSourceV3], EditorError> {
    struct Metadata {
        profile: FontRenderProfileV3,
        license_hash: String,
    }
    static META: std::sync::OnceLock<Result<Vec<Metadata>, EditorError>> =
        std::sync::OnceLock::new();
    static SOURCES: std::sync::OnceLock<Result<Vec<ApprovedFontSourceV3>, EditorError>> =
        std::sync::OnceLock::new();
    let error = || {
        EditorError::new(
            "FONT_SOURCE_INTEGRITY_ERROR",
            "Compiled derived render identity is invalid",
        )
    };
    SOURCES
        .get_or_init(|| {
            let pins = crate::font_derived_v2::compiled_sources_v2()?;
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
                    let source = &pin.source;
                    let size_bytes = source.face.size_bytes.parse().map_err(|_| error())?;
                    Ok(ApprovedFontSourceV3 {
                        render_profile: metadata.profile,
                        source: PinnedFontSourceV1 {
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
