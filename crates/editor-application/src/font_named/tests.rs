use super::*;
use opencut_contracts::edit_recovery::*;
use opencut_contracts::{CreateProject, FrameRate};
fn request(id: &str) -> ImportFont {
    ImportFont {
        project_id: "p".into(),
        expected_revision: "r".into(),
        idempotency_key: "original-import".into(),
        catalog_id: id.into(),
        font_id: "font-v2".into(),
        required_text: "AV".into(),
    }
}
fn resign(source: &mut FontDerivedSource, mut value: Value) {
    value.as_object_mut().unwrap().remove("derivation_id");
    let id = format!(
        "font-derived-v2_{:x}",
        Sha256::digest(validation::canonical(&value).unwrap().as_bytes())
    );
    value["derivation_id"] = Value::String(id.clone());
    source.face.derivation_id = id;
    source.proof_json = validation::canonical(&value).unwrap();
}
#[test]
fn compiled_recipes_keep_opaque_native_family_and_original_display_notice_separate() {
    let pins = compiled_sources().unwrap();
    assert!(!pins.is_empty());
    for pin in pins {
        validate_pinned_source(pin).unwrap();
        let proof: Value = serde_json::from_str(&pin.source.proof_json).unwrap();
        assert_eq!(
            display_family(&pin.source.face.id).unwrap(),
            proof["source"]["source_family"].as_str().unwrap()
        );
        if pin.source.license_id == "Ubuntu-font-1.0" {
            assert!(pin.source.face.family.starts_with(&format!(
                "{} derivative OCDF ",
                display_family(&pin.source.face.id).unwrap()
            )));
        } else {
            assert!(pin.source.face.family.starts_with("OCDF "));
        }
        assert_ne!(
            display_family(&pin.source.face.id).unwrap(),
            pin.source.face.family
        );
        assert_eq!(pin.source.license_notice, proof["license_notice"]);
        assert!(crate::font_derived::validate_pinned_source(pin).is_err());
        assert!(matches!(
            render_profile(&pin.source.face.id).unwrap().as_str(),
            "static_outline" | "colr_v0" | "colr_v1" | "monochrome_bitmap"
        ));
        crate::font_derived::provenance::validate_source(&pin.source).unwrap();
    }
}
#[test]
fn resealed_tampering_cannot_keep_reserved_identity_or_change_version_profile() {
    let original = compiled_sources().unwrap()[0].source.clone();
    let mut changed = original.clone();
    let mut proof: Value = serde_json::from_str(&changed.proof_json).unwrap();
    proof["name_rewrite"]["output_identity_names"][0]["value"] = serde_json::json!("Source");
    resign(&mut changed, proof);
    assert!(validate_pinned_source(&PinnedDerivedFont { source: changed }).is_err());
    let mut changed = original.clone();
    let mut proof: Value = serde_json::from_str(&changed.proof_json).unwrap();
    proof["source"]["render_profile"] = serde_json::json!("unbounded-svg");
    resign(&mut changed, proof);
    assert!(validate_pinned_source(&PinnedDerivedFont { source: changed }).is_err());
    let mut changed = original;
    changed.face.family = "Assistant".into();
    assert!(validate_pinned_source(&PinnedDerivedFont { source: changed }).is_err());
}
#[test]
fn v1_recovery_envelope_preserves_original_v2_import_without_changing_v1_admission() {
    let pin = &compiled_sources().unwrap()[0];
    let request = request(&pin.source.face.id);
    assert!(crate::font_derived::validate_import_request(&request).is_err());
    crate::font_derived::provenance::validate_import_request(&request).unwrap();
    let font = crate::font_derived::provenance::expected_import_resource(&request).unwrap();
    assert_eq!(font.family, pin.source.face.family);
    assert!(
        matches!(font.origin,ResourceOrigin::Generated{provider,..} if provider==NAMED_FONT_PROVIDER)
    );
    let input = StageEditRecovery {
        context: EditRecoveryContext {
            intent: opencut_contracts::edit_recovery::EditRecoveryIntent::FontImport {
                request: request.clone(),
            },
            sequence_id: Some("s".into()),
        },
    };
    let prepared = crate::edit_recovery::prepare(&input).unwrap();
    assert_eq!(
        crate::edit_recovery::decode_payload(prepared.payload(), prepared.digest(), "p").unwrap(),
        input
    );
}
#[test]
fn provider_and_proof_versions_cannot_be_cross_wrapped() {
    let pin = &compiled_sources().unwrap()[0];
    let mut font = expected_import_resource(&request(&pin.source.face.id)).unwrap();
    proof::validate_resource_source(&font, &pin.source).unwrap();
    if let ResourceOrigin::Generated { provider, .. } = &mut font.origin {
        *provider = crate::font_derived::OUTLINE_FONT_PROVIDER.into();
    }
    assert!(crate::font_derived::provenance::validate_resource_source(&font, &pin.source).is_err());
    let old = crate::font_derived::catalog().unwrap().remove(0);
    let mut old_request = request(&old.source.face.id);
    old_request.font_id = "old".into();
    let mut font = crate::font_derived::expected_import_resource(&old_request).unwrap();
    if let ResourceOrigin::Generated { provider, .. } = &mut font.origin {
        *provider = NAMED_FONT_PROVIDER.into();
    }
    assert!(crate::font_derived::provenance::validate_resource_source(&font, &old.source).is_err());
}
#[test]
fn package6_and_history7_keep_both_proof_versions_and_require_exact_inventory() {
    use crate::project_archive::*;
    let mut doc = crate::scene::new_document(&CreateProject {
        id: "p".into(),
        name: "p".into(),
        sequence_id: "s".into(),
        width: 64,
        height: 48,
        fps: FrameRate {
            numerator: 25,
            denominator: 1,
        },
    })
    .unwrap();
    let old = crate::font_derived::catalog().unwrap().remove(0).source;
    let new = compiled_sources().unwrap()[0].source.clone();
    let mut old_request = request(&old.face.id);
    old_request.font_id = "font-v1".into();
    doc.fonts = vec![
        crate::font_derived::expected_import_resource(&old_request).unwrap(),
        expected_import_resource(&request(&new.face.id)).unwrap(),
    ];
    let mut sources = vec![old, new];
    sources.sort_by(|a, b| a.face.derivation_id.cmp(&b.face.derivation_id));
    crate::historical_snapshot_package::validate_export_source(
        &crate::historical_snapshot_package::HistoricalSnapshotSource {
            snapshot: EditorSnapshot {
                project: doc.clone(),
                revision: "r".into(),
            },
            derived_fonts: sources.clone(),
        },
    )
    .unwrap();
    let source = ArchiveSource {
        archive: ProjectArchive {
            schema_version: 1,
            source_project_id: "p".into(),
            main_head_revision: "r".into(),
            revisions: vec![ArchivedRevision {
                revision: "r".into(),
                parent_revision: None,
                transaction_id: None,
                document: doc.into(),
                created_at: "2026-10-16 00:00:00".into(),
            }],
            transactions: vec![],
            branches: vec![],
            merges: vec![],
        },
        derived_fonts: sources,
    };
    validate_source(&source).unwrap();
    let manifest = make_manifest(&source).unwrap();
    assert_eq!(manifest.schema_version, 7);
    assert_eq!(manifest.derived_fonts.len(), 2);
    let mut missing = source.clone();
    missing.derived_fonts.pop();
    assert!(validate_source(&missing).is_err());
}
#[test]
fn static_corrections_are_explicit_and_large_source_does_not_raise_output_budget() {
    let pins = compiled_sources().unwrap();
    let mut corrected = 0;
    let mut large = 0;
    let mut ubuntu = 0;
    for pin in pins {
        let proof: Value = serde_json::from_str(&pin.source.proof_json).unwrap();
        let kind = proof["source"]["recipe_kind"].as_str().unwrap();
        if kind.starts_with("static_") {
            corrected += 1;
            assert!(pin.source.axes.is_empty());
            assert!(
                proof["static_correction"]["preserved_source_tables"]
                    .get("glyf")
                    .is_some()
            );
            assert_eq!(
                proof["static_correction"]["target_weight"],
                pin.source.face.weight
            );
        }
        if pin.source.source_size_bytes.parse::<u64>().unwrap() > MAX_FONT_BYTES {
            large += 1;
            assert_eq!(
                display_family(&pin.source.face.id).unwrap(),
                "Chiron GoRound TC"
            );
            assert!(pin.source.face.size_bytes.parse::<u64>().unwrap() <= MAX_FONT_BYTES);
        }
        if pin.source.license_id == "Ubuntu-font-1.0" {
            ubuntu += 1;
            assert_eq!(
                proof["source"]["name_rewrite_version"],
                "opencut-ubuntu-derivative-names-v1"
            );
            assert!(
                proof["source"]["derivative_notice"]
                    .as_str()
                    .unwrap()
                    .contains("Modified Version")
            );
        }
    }
    assert_eq!(corrected, 18);
    assert_eq!(large, 2);
    assert_eq!(ubuntu, 8);
}
