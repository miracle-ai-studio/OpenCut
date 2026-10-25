use super::*;
use crate::font_import::{self as old, catalog::*};
use opencut_contracts::{font_catalog::*, font_import::ImportFont};
use sha2::{Digest, Sha256};

fn request() -> ImportFont {
    ImportFont {
        project_id: "project".into(),
        expected_revision: "rev".into(),
        idempotency_key: "key".into(),
        catalog_id: "google-fjordone-regular-alias-v3".into(),
        font_id: "fjord".into(),
        required_text: "Fjord".into(),
    }
}
#[test]
fn explicit_alias_preserves_native_identity_bytes_and_license() {
    let request = request();
    let pin = native_source(&request.catalog_id).unwrap();
    let entry = source(&FontSourceRequest {
        catalog_id: request.catalog_id.clone(),
    })
    .unwrap();
    assert_eq!(entry.face.display_family, "Fjord One");
    assert_eq!(entry.face.family, "Fjord");
    assert!(
        entry
            .face
            .alias_reason
            .unwrap()
            .contains("Bytes and native family are preserved")
    );
    let resource = expected_import_resource(&request).unwrap();
    assert_eq!(resource.family, "Fjord");
    assert_eq!(resource.content.content_hash, pin.source.content_hash);
    assert_eq!(resource.content.size, pin.source.size_bytes);
    assert_eq!(
        resource.license.unwrap().notice.as_deref(),
        Some(pin.source.license_notice)
    );
    assert_eq!(
        old::expected_import_resource(&request).unwrap(),
        expected_import_resource(&request).unwrap()
    );
}
#[test]
fn source_identity_selects_its_approved_route_and_rejects_unknown_pins() {
    let request = request();
    assert!(pinned_font_source(&request.catalog_id).is_err());
    assert!(
        source(&FontSourceRequest {
            catalog_id: "unknown-unapproved-font".into()
        })
        .is_err()
    );
    assert!(native_source("google-fjordone-regular-v1").is_err());
    assert!(native_source("google-pushster-regular-alias-v3").is_err());
    assert!(native_source("google-inconsolata-regular-alias-v3").is_err());
}
#[test]
fn approved_lookup_requires_exact_hash_size_and_declares_color_profile() {
    let pin = native_source("google-anonymouspro-bold-color-v3").unwrap();
    assert_eq!(pin.render_profile, FontRenderProfile::MonochromeBitmap);
    assert_eq!(
        approved_native_source(pin.source.content_hash, pin.source.size_bytes)
            .unwrap()
            .source
            .catalog_id,
        pin.source.catalog_id
    );
    assert!(approved_native_source(pin.source.content_hash, pin.source.size_bytes + 1).is_none());
    assert!(approved_native_source(&"0".repeat(64), pin.source.size_bytes).is_none());
}
#[test]
fn source_inventory_is_unique_bounded_and_notice_digest_is_exact() {
    let mut ids = std::collections::BTreeSet::new();
    for entry in static_sources() {
        let pin = &entry.approved.source;
        assert!(ids.insert(pin.catalog_id));
        assert!(pin.size_bytes > 0 && pin.size_bytes <= old::MAX_FONT_BYTES);
        assert_eq!(
            format!("{:x}", Sha256::digest(pin.license_notice.as_bytes())),
            pin.license_sha256
        );
        assert_eq!(pin.source_commit.len(), 40);
        assert!(pin.source_commit.bytes().all(|b| b.is_ascii_hexdigit()));
        assert!(pin.source_url.contains(pin.source_commit));
        assert!(!entry.display_family.is_empty());
    }
    assert_eq!(STATIC_SOURCES.len(), 32);
    assert_eq!(COLOR_SOURCES.len(), 12);
}
#[test]
fn original_request_recovery_accepts_exact_historical_visual_pin_and_preserves_request() {
    use opencut_contracts::edit_recovery::*;
    let request = StageEditRecovery {
        context: EditRecoveryContext {
            intent: opencut_contracts::edit_recovery::EditRecoveryIntent::FontImport {
                request: request(),
            },
            sequence_id: Some("scene".into()),
        },
    };
    let prepared = crate::edit_recovery::prepare(&request).unwrap();
    let reopened =
        crate::edit_recovery::decode_payload(prepared.payload(), prepared.digest(), "project")
            .unwrap();
    assert_eq!(reopened.context, request.context);
    let mut invalid = request;
    let EditRecoveryIntent::FontImport { request } = &mut invalid.context.intent else {
        panic!()
    };
    request.catalog_id = "google-fjordone-regular-alias-v3-unpinned".into();
    assert!(crate::edit_recovery::prepare(&invalid).is_err());
}
#[test]
fn download_permission_and_budget_are_not_catalog_request_fields() {
    let request = request();
    assert_eq!(
        validate_fetch(&request.catalog_id, &FontFetchPolicy::default())
            .unwrap_err()
            .code,
        "FONT_NETWORK_DENIED"
    );
    let mut policy = FontFetchPolicy {
        allow_public_font_download: true,
        max_bytes: 1,
        ..FontFetchPolicy::default()
    };
    assert_eq!(
        validate_fetch(&request.catalog_id, &policy)
            .unwrap_err()
            .code,
        "FONT_BUDGET_EXCEEDED"
    );
    policy.max_bytes = old::MAX_FONT_BYTES;
    policy.deadline_ms = 30001;
    assert_eq!(
        validate_fetch(&request.catalog_id, &policy)
            .unwrap_err()
            .code,
        "INVALID_ARGUMENT"
    );
}

#[test]
fn ufl_is_an_explicit_historical_visual_source_and_keeps_the_complete_original_notice() {
    let mut input = request();
    input.catalog_id = "google-ubuntu-regular-source-v3".into();
    let pin = native_source(&input.catalog_id).unwrap();
    assert_eq!(pin.source.license_identifier, "Ubuntu-font-1.0");
    assert!(
        pin.source
            .license_notice
            .contains("UBUNTU FONT LICENCE Version 1.0")
    );
    assert!(
        old::validate_font_spec(
            &pin.source
                .import_spec(input.font_id.clone(), input.required_text.clone())
        )
        .is_err()
    );
    let font = expected_import_resource(&input).unwrap();
    assert_eq!(font.family, "Ubuntu");
    assert_eq!(
        font.license.unwrap().identifier.as_deref(),
        Some("Ubuntu-font-1.0")
    );
    assert!(pinned_font_source(&input.catalog_id).is_err());
}

#[test]
fn historical_same_name_source_and_separate_upstream_license_keep_full_pins() {
    let entry = source(&FontSourceRequest {
        catalog_id: "google-sourcesanspro-regular-source-v3".into(),
    })
    .unwrap();
    assert_eq!(entry.face.family, "Source Sans Pro");
    let FontSourceDetails::Static(pin) = entry.details else {
        panic!("static source");
    };
    assert_eq!(
        pin.source_commit,
        "a7bc06df72f7e1b4e20342e2b04423e579222316"
    );
    let tinos = source(&FontSourceRequest {
        catalog_id: "google-tinos-regular-source-v3".into(),
    })
    .unwrap();
    assert!(
        tinos
            .license_url
            .contains("googlefonts/tinos/3b4482a99b80ea5fc75f187b1be3120a3f5905b3/")
    );
    let FontSourceDetails::Static(pin) = tinos.details else {
        panic!("static source");
    };
    assert_eq!(
        tinos.license_sha256,
        format!("{:x}", Sha256::digest(pin.license_notice.as_bytes()))
    );
}

#[test]
fn larger_policy_does_not_admit_unknown_large_bytes_or_expand_text_controls() {
    let policy = old::FontImportPolicy {
        max_font_bytes: MAX_CATALOG_FONT_BYTES,
        ..old::FontImportPolicy::default()
    };
    assert_eq!(
        validate_catalog_font_envelope(&"0".repeat(64), old::MAX_FONT_BYTES + 1, "text", &policy)
            .unwrap_err()
            .code,
        "FONT_BUDGET_EXCEEDED"
    );
    assert!(validate_catalog_font_envelope("unused", 1, "x\0y", &policy).is_err());
    assert!(
        validate_catalog_font_envelope(
            "unused",
            2,
            "text",
            &old::FontImportPolicy {
                max_font_bytes: 1,
                ..policy
            }
        )
        .is_err()
    );
}
