//! Portable binding/recovery checks; these do not execute a browser font or CAS.
use super::*;
use crate::font_synthesis_v1 as synthesis;
use opencut_contracts::{CreateProject, FrameRate};
fn request(id: &str) -> ImportDerivedFontV1 {
    ImportDerivedFontV1 {
        project_id: "p".into(),
        expected_revision: "r".into(),
        idempotency_key: "original-synthesis-import".into(),
        catalog_id: id.into(),
        font_id: "synthetic-font".into(),
        required_text: "AV".into(),
    }
}
#[test]
fn private_dispatch_preserves_actual_face_and_explicit_requested_style() {
    let catalog = synthesis::catalog().unwrap();
    assert!(!catalog.is_empty());
    for entry in catalog {
        let source = entry.source;
        let input = request(&source.face.id);
        assert!(source.face.id.ends_with("-synthetic-italic-v1"));
        assert_eq!(source.face.derivation_id, entry.binding.binding_id);
        assert_eq!(source.face.weight, entry.binding.actual.weight);
        assert_eq!(source.face.style, entry.binding.actual.style);
        assert_eq!(source.face.style, FontStyleV4::Normal);
        let binding = serde_json::to_value(&entry.binding).unwrap();
        assert_eq!(binding["requested"]["weight"], 700);
        assert_eq!(binding["requested"]["style"], "italic");
        assert!(crate::font_derived_v1::validate_import_request_v1(&input).is_err());
        assert!(super::validate_import_request_v2(&input).is_err());
        compat::validate_import_request(&input).unwrap();
        assert_eq!(compat::source(&source.face.id).unwrap().source, source);
        compat::validate_source(&source).unwrap();
        let font = compat::expected_import_resource(&input).unwrap();
        assert_eq!(font.style, FontStyleV4::Normal);
        assert_eq!(font.content.content_hash, source.face.content_hash);
        assert_eq!(compat::proof_schema_version(&font, &source).unwrap(), 3);
        assert_eq!(
            synthesis::binding_for_resource(&font).unwrap(),
            Some(entry.binding)
        );
        for provider in [
            crate::font_derived_v1::DERIVED_FONT_PROVIDER_V1,
            DERIVED_FONT_PROVIDER_V2,
        ] {
            let mut forged = font.clone();
            let ResourceOriginV4::Generated { provider: p, .. } = &mut forged.origin else {
                panic!()
            };
            *p = provider.into();
            assert!(compat::validate_resource_source(&forged, &source).is_err());
            assert!(compat::proof_schema_version(&forged, &source).is_err());
        }
    }
}
#[test]
fn original_synthesis_request_uses_unchanged_recovery_envelope_and_exact_key() {
    use crate::font_derived_v1::recovery;
    let entry = synthesis::catalog().unwrap().remove(0);
    let input = StageDerivedFontRecoveryV1 {
        context: DerivedFontRecoveryContextV1 {
            request: request(&entry.source.face.id),
            sequence_id: Some("s".into()),
        },
    };
    let prepared = recovery::prepare(&input).unwrap();
    assert_eq!(
        recovery::decode_payload(prepared.payload(), prepared.digest(), "p").unwrap(),
        input
    );
    let mut changed = input.clone();
    changed.context.request.required_text.push('X');
    assert_ne!(
        recovery::prepare(&changed).unwrap().digest(),
        prepared.digest()
    );
    assert_eq!(
        input.context.request.idempotency_key,
        "original-synthesis-import"
    );
}
#[test]
fn package6_and_history7_keep_three_typed_proof_versions_without_new_envelope() {
    use crate::project_history_package_v1::*;
    let mut document = crate::project_v4::new_document(&CreateProject {
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
    let v1 = crate::font_derived_v1::catalog().unwrap().remove(0).source;
    let v2 = compiled_sources_v2().unwrap()[0].source.clone();
    let v3 = synthesis::catalog().unwrap().remove(0).source;
    let mut sources = vec![v1, v2, v3];
    for (index, source) in sources.iter().enumerate() {
        let mut input = request(&source.face.id);
        input.font_id = format!("font-proof-{}", index + 1);
        document
            .fonts
            .push(compat::expected_import_resource(&input).unwrap());
    }
    sources.sort_by(|a, b| a.face.derivation_id.cmp(&b.face.derivation_id));
    let roundtrip: Vec<FontDerivedSourceV1> =
        serde_json::from_str(&serde_json::to_string(&sources).unwrap()).unwrap();
    assert_eq!(roundtrip, sources);
    crate::project_package_v4::validate_export_source(
        &crate::project_package_v4::PackageExportSourceV4 {
            snapshot: ProjectSnapshotV4 {
                project: document.clone(),
                revision: "r".into(),
            },
            derived_fonts: sources.clone(),
        },
    )
    .unwrap();
    let history = HistoryPackageSourceV1 {
        archive: HistoryArchiveV1 {
            schema_version: 1,
            source_project_id: "p".into(),
            main_head_revision: "r".into(),
            revisions: vec![HistoryRevisionV1 {
                revision: "r".into(),
                parent_revision: None,
                transaction_id: None,
                document,
                created_at: "2026-10-16 00:00:00".into(),
            }],
            transactions: vec![],
            branches: vec![],
            merges: vec![],
        },
        derived_fonts: sources,
    };
    validate_source(&history).unwrap();
    let manifest = make_manifest(&history).unwrap();
    assert_eq!(manifest.schema_version, 7);
    assert_eq!(manifest.derived_fonts.len(), 3);
    let mut missing = history.clone();
    missing
        .derived_fonts
        .retain(|s| !s.face.id.ends_with("-synthetic-italic-v1"));
    assert!(validate_source(&missing).is_err());
}
