use super::*;
use crate::font_import_v1::{self as old, catalog::*};
use opencut_contracts::{font_catalog_v3::*, font_import_v1::ImportFontV1};
use sha2::{Digest, Sha256};

fn request() -> ImportFontV1 {
    ImportFontV1 {
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
    let pin = native_source_v3(&request.catalog_id).unwrap();
    let entry = source(&FontSourceRequestV3 {
        catalog_id: request.catalog_id.clone(),
        kind: FontCatalogKindV3::StaticV3,
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
    let resource = expected_import_resource_v3(&request).unwrap();
    assert_eq!(resource.family, "Fjord");
    assert_eq!(resource.content.content_hash, pin.source.content_hash);
    assert_eq!(resource.content.size, pin.source.size_bytes);
    assert_eq!(
        resource.license.unwrap().notice.as_deref(),
        Some(pin.source.license_notice)
    );
    assert_eq!(
        old::expected_import_resource_v1(&request).unwrap(),
        expected_import_resource_v3(&request).unwrap()
    );
}
#[test]
fn old_source_and_wrong_typed_route_do_not_certify_v3_pins() {
    let request = request();
    assert!(pinned_font_source_v1(&request.catalog_id).is_err());
    for kind in [
        FontCatalogKindV3::StaticV1,
        FontCatalogKindV3::DerivedV1,
        FontCatalogKindV3::DerivedV2,
    ] {
        assert!(
            source(&FontSourceRequestV3 {
                catalog_id: request.catalog_id.clone(),
                kind
            })
            .is_err()
        );
    }
    assert!(native_source_v3("google-fjordone-regular-v1").is_err());
    assert!(native_source_v3("google-pushster-regular-alias-v3").is_err());
    assert!(native_source_v3("google-inconsolata-regular-alias-v3").is_err());
}
#[test]
fn approved_lookup_requires_exact_hash_size_and_declares_color_profile() {
    let pin = native_source_v3("google-anonymouspro-bold-color-v3").unwrap();
    assert_eq!(pin.render_profile, FontRenderProfileV3::MonochromeBitmap);
    assert_eq!(
        approved_native_source_v3(pin.source.content_hash, pin.source.size_bytes)
            .unwrap()
            .source
            .catalog_id,
        pin.source.catalog_id
    );
    assert!(
        approved_native_source_v3(pin.source.content_hash, pin.source.size_bytes + 1).is_none()
    );
    assert!(approved_native_source_v3(&"0".repeat(64), pin.source.size_bytes).is_none());
}
#[test]
fn source_inventory_is_unique_bounded_and_notice_digest_is_exact() {
    let mut ids = std::collections::BTreeSet::new();
    for entry in static_sources_v3() {
        let pin = &entry.approved.source;
        assert!(ids.insert(pin.catalog_id));
        assert!(pin.size_bytes > 0 && pin.size_bytes <= old::MAX_FONT_BYTES_V1);
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
fn original_request_recovery_accepts_exact_v3_pin_and_preserves_request() {
    use opencut_contracts::font_recovery_v1::*;
    let request = StageFontRecoveryV1 {
        context: FontRecoveryContextV1 {
            request: request(),
            sequence_id: Some("scene".into()),
        },
    };
    let prepared = old::recovery::prepare(&request).unwrap();
    let reopened =
        old::recovery::decode_payload(prepared.payload(), prepared.digest(), "project").unwrap();
    assert_eq!(reopened.context.request, request.context.request);
    let mut invalid = request;
    invalid.context.request.catalog_id = "google-fjordone-regular-alias-v3-unpinned".into();
    assert!(old::recovery::prepare(&invalid).is_err());
}
#[test]
fn download_permission_and_budget_are_not_catalog_request_fields() {
    let request = request();
    assert_eq!(
        validate_fetch_v3(&request.catalog_id, &FontFetchPolicyV1::default())
            .unwrap_err()
            .code,
        "FONT_NETWORK_DENIED"
    );
    let mut policy = FontFetchPolicyV1 {
        allow_public_font_download: true,
        max_bytes: 1,
        ..FontFetchPolicyV1::default()
    };
    assert_eq!(
        validate_fetch_v3(&request.catalog_id, &policy)
            .unwrap_err()
            .code,
        "FONT_BUDGET_EXCEEDED"
    );
    policy.max_bytes = old::MAX_FONT_BYTES_V1;
    policy.deadline_ms = 30001;
    assert_eq!(
        validate_fetch_v3(&request.catalog_id, &policy)
            .unwrap_err()
            .code,
        "INVALID_ARGUMENT"
    );
}

#[test]
fn ufl_is_an_explicit_v3_source_and_keeps_the_complete_original_notice() {
    let mut input = request();
    input.catalog_id = "google-ubuntu-regular-source-v3".into();
    let pin = native_source_v3(&input.catalog_id).unwrap();
    assert_eq!(pin.source.license_identifier, "Ubuntu-font-1.0");
    assert!(
        pin.source
            .license_notice
            .contains("UBUNTU FONT LICENCE Version 1.0")
    );
    assert!(
        old::validate_font_spec_v1(
            &pin.source
                .import_spec(input.font_id.clone(), input.required_text.clone())
        )
        .is_err()
    );
    let font = expected_import_resource_v3(&input).unwrap();
    assert_eq!(font.family, "Ubuntu");
    assert_eq!(
        font.license.unwrap().identifier.as_deref(),
        Some("Ubuntu-font-1.0")
    );
    assert!(pinned_font_source_v1(&input.catalog_id).is_err());
}

#[test]
fn historical_same_name_source_and_separate_upstream_license_keep_full_pins() {
    let entry = source(&FontSourceRequestV3 {
        catalog_id: "google-sourcesanspro-regular-source-v3".into(),
        kind: FontCatalogKindV3::StaticV3,
    })
    .unwrap();
    assert_eq!(entry.face.family, "Source Sans Pro");
    let FontSourceDetailsV3::StaticV3(pin) = entry.details else {
        panic!("static source");
    };
    assert_eq!(
        pin.source_commit,
        "a7bc06df72f7e1b4e20342e2b04423e579222316"
    );
    let tinos = source(&FontSourceRequestV3 {
        catalog_id: "google-tinos-regular-source-v3".into(),
        kind: FontCatalogKindV3::StaticV3,
    })
    .unwrap();
    assert!(
        tinos
            .license_url
            .contains("googlefonts/tinos/3b4482a99b80ea5fc75f187b1be3120a3f5905b3/")
    );
    let FontSourceDetailsV3::StaticV3(pin) = tinos.details else {
        panic!("static source");
    };
    assert_eq!(
        tinos.license_sha256,
        format!("{:x}", Sha256::digest(pin.license_notice.as_bytes()))
    );
}

#[test]
fn larger_policy_does_not_admit_unknown_large_bytes_or_expand_text_controls() {
    let policy = old::FontImportPolicyV1 {
        max_font_bytes: MAX_CATALOG_FONT_BYTES_V3,
        ..old::FontImportPolicyV1::default()
    };
    assert_eq!(
        validate_catalog_font_envelope_v3(
            &"0".repeat(64),
            old::MAX_FONT_BYTES_V1 + 1,
            "text",
            &policy
        )
        .unwrap_err()
        .code,
        "FONT_BUDGET_EXCEEDED"
    );
    assert!(validate_catalog_font_envelope_v3("unused", 1, "x\0y", &policy).is_err());
    assert!(
        validate_catalog_font_envelope_v3(
            "unused",
            2,
            "text",
            &old::FontImportPolicyV1 {
                max_font_bytes: 1,
                ..policy
            }
        )
        .is_err()
    );
}
