//! Fixed official CSS distributions live in the existing font catalog/use case.
//! A distribution release is not a Git commit; frozen Git discovery stays exact.
use super::*;
use crate::font_import::{catalog::*, *};
use crate::scene::SceneStore;
use opencut_contracts::edit_recovery::*;
use opencut_contracts::{
    TransactionResult, font_catalog::*, font_import::ImportFont, scene::FontResource,
};

// source_commit is intentionally empty in these private reusable fetch pins.
// They never enter static_sources or the frozen Git source DTOs.
static SOURCES: &[StaticFontEntry] = include!("distribution_generated.rs");

pub(crate) fn contains_source(id: &str) -> bool {
    SOURCES
        .iter()
        .any(|entry| entry.approved.source.catalog_id == id)
}
pub fn native_source(id: &str) -> Result<&'static ApprovedFontSource, EditorError> {
    SOURCES
        .iter()
        .find(|entry| entry.approved.source.catalog_id == id)
        .map(|entry| &entry.approved)
        .ok_or_else(|| EditorError::new("NOT_FOUND", "Unknown fixed font distribution"))
}
pub(super) fn approved_native_source(hash: &str, size: u64) -> Option<&'static ApprovedFontSource> {
    SOURCES
        .iter()
        .find(|entry| {
            entry.approved.source.content_hash == hash && entry.approved.source.size_bytes == size
        })
        .map(|entry| &entry.approved)
}
fn face(entry: &StaticFontEntry) -> FontFace {
    let pin = &entry.approved.source;
    FontFace {
        id: pin.catalog_id.into(),
        display_family: entry.display_family.into(),
        family: pin.family.into(),
        weight: pin.weight,
        style: pin.style,
        requested_style: None,
        kind: FontKind::Distribution,
        content_hash: pin.content_hash.into(),
        size_bytes: pin.size_bytes.to_string(),
        render_profile: entry.approved.render_profile,
        alias_reason: entry.alias_reason.map(Into::into),
        import_available: false,
        unavailable_reason: Some("Host source availability has not been attached.".into()),
    }
}
pub fn catalog() -> Vec<FontFace> {
    SOURCES.iter().map(face).collect()
}
pub fn source(request: &FontSourceRequest) -> Result<FontDistributionSource, EditorError> {
    use std::sync::OnceLock;
    #[derive(serde::Deserialize)]
    struct Identity {
        catalog_id: String,
    }
    #[derive(serde::Deserialize)]
    struct Record {
        source: Identity,
        distribution: FontDistributionProvenance,
    }
    static PROOFS: OnceLock<Result<Vec<Record>, EditorError>> = OnceLock::new();
    let entry = SOURCES
        .iter()
        .find(|entry| entry.approved.source.catalog_id == request.catalog_id)
        .ok_or_else(|| EditorError::new("NOT_FOUND", "Unknown fixed font distribution"))?;
    let pin = &entry.approved.source;
    let records = PROOFS
        .get_or_init(|| {
            serde_json::from_str(include_str!("distribution_source_pins.json")).map_err(|_| {
                EditorError::new(
                    "FONT_SOURCE_INTEGRITY_ERROR",
                    "Invalid compiled distribution provenance",
                )
            })
        })
        .as_ref()
        .map_err(Clone::clone)?;
    let proof = records
        .iter()
        .find(|record| record.source.catalog_id == pin.catalog_id)
        .ok_or_else(|| {
            EditorError::new(
                "FONT_SOURCE_INTEGRITY_ERROR",
                "Missing fixed distribution provenance",
            )
        })?;
    if proof.distribution.source_url != pin.source_url {
        return Err(EditorError::new(
            "FONT_SOURCE_INTEGRITY_ERROR",
            "Distribution source identity differs",
        ));
    }
    Ok(FontDistributionSource {
        face: face(entry),
        provenance: proof.distribution.clone(),
        license_id: pin.license_identifier.into(),
        license_notice: pin.license_notice.into(),
        license_url: pin.license_url.into(),
        license_sha256: pin.license_sha256.into(),
    })
}
pub fn validate_request(request: &ImportFont) -> Result<(), EditorError> {
    super::import::validate_import_identity(request)?;
    super::import::validate_selected_request(request, native_source(&request.catalog_id)?)
}
pub fn validate_fetch(
    id: &str,
    policy: &FontFetchPolicy,
) -> Result<&'static ApprovedFontSource, EditorError> {
    let source = native_source(id)?;
    super::import::validate_selected_fetch(source, policy)?;
    Ok(source)
}
pub fn expected_resource(request: &ImportFont) -> Result<FontResource, EditorError> {
    super::import::expected_selected_resource(request, native_source(&request.catalog_id)?)
}
pub fn import(
    store: &mut impl SceneStore,
    fetcher: &impl FontSourceFetch,
    inspector: &impl FontInspector,
    publisher: &impl FontCasPublish,
    request: &ImportFont,
    fetch_policy: &FontFetchPolicy,
    inspection_policy: &FontImportPolicy,
    checkpoint: &mut dyn FontImportCheckpoint,
) -> Result<TransactionResult, EditorError> {
    super::import::import_selected_static_font(
        store,
        fetcher,
        inspector,
        publisher,
        request,
        fetch_policy,
        inspection_policy,
        checkpoint,
        native_source(&request.catalog_id)?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    #[test]
    fn distributions_keep_native_identity_complete_license_and_original_recovery() {
        assert_eq!(SOURCES.len(), 3);
        for entry in SOURCES {
            let pin = &entry.approved.source;
            assert!(pin.source_commit.is_empty());
            assert!(super::super::native_source(pin.catalog_id).is_err());
            assert!(pinned_font_source(pin.catalog_id).is_err());
            let details = source(&FontSourceRequest {
                catalog_id: pin.catalog_id.into(),
            })
            .unwrap();
            assert_eq!(details.provenance.source_url, pin.source_url);
            assert!(
                details
                    .provenance
                    .css_url
                    .starts_with("https://fonts.googleapis.com/css2?")
            );
            assert_eq!(
                format!("{:x}", Sha256::digest(pin.license_notice.as_bytes())),
                details.license_sha256
            );
            let request = ImportFont {
                project_id: "p".into(),
                expected_revision: "rev".into(),
                idempotency_key: "key".into(),
                catalog_id: pin.catalog_id.into(),
                font_id: "font".into(),
                required_text: "home".into(),
            };
            assert_eq!(
                expected_resource(&request).unwrap(),
                crate::font_import::expected_import_resource(&request).unwrap()
            );
            let staged = opencut_contracts::edit_recovery::StageEditRecovery {
                context: opencut_contracts::edit_recovery::EditRecoveryContext {
                    intent: opencut_contracts::edit_recovery::EditRecoveryIntent::FontImport {
                        request: request.clone(),
                    },
                    sequence_id: Some("scene".into()),
                },
            };
            let prepared = crate::edit_recovery::prepare(&staged).unwrap();
            assert_eq!(
                crate::edit_recovery::decode_payload(prepared.payload(), prepared.digest(), "p")
                    .unwrap()
                    .context
                    .intent,
                opencut_contracts::edit_recovery::EditRecoveryIntent::FontImport { request }
            );
            assert_eq!(
                validate_fetch(pin.catalog_id, &FontFetchPolicy::default())
                    .unwrap_err()
                    .code,
                "FONT_NETWORK_DENIED"
            );
            if entry.display_family == "Pushster" {
                assert_eq!(details.face.family, "Lobster");
                assert!(details.face.alias_reason.unwrap().contains("official CSS"));
            }
        }
        assert!(native_source("https://fonts.gstatic.com/unpinned.ttf").is_err());
    }
}
