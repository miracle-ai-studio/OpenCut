use super::*;
use opencut_contracts::{CreateProject, FrameRate, project_v4_history::*, project_v4_ops::*};
fn source() -> HistoryPackageSourceV1 {
    let doc = crate::project_v4::new_document(&CreateProject {
        id: "source".into(),
        name: "source".into(),
        sequence_id: "scene".into(),
        width: 64,
        height: 48,
        fps: FrameRate {
            numerator: 30,
            denominator: 1,
        },
    })
    .unwrap();
    let mut a = HistoryArchiveV1 {
        schema_version: 1,
        source_project_id: "source".into(),
        main_head_revision: "r4".into(),
        revisions: vec![HistoryRevisionV1 {
            revision: "r0".into(),
            parent_revision: None,
            transaction_id: None,
            document: doc.clone(),
            created_at: "2026-01-01 00:00:00".into(),
        }],
        transactions: vec![],
        branches: vec![HistoryBranchV1 {
            name: "saved".into(),
            head_revision: "r1".into(),
            created_from_revision: "r0".into(),
            created_at: "2026-01-01 00:00:00".into(),
        }],
        merges: vec![HistoryMergeV1 {
            revision: "r4".into(),
            source_revision: "r1".into(),
        }],
    };
    for i in 1..=4 {
        let revision = format!("r{i}");
        let base = format!("r{}", i - 1);
        let transaction = format!("t{i}");
        let mut document = doc.clone();
        document.name = if i == 2 { "source" } else { "r0" }.into();
        let journal = match i {
            2 => HistoryPackageJournalV1::Causal(CausalJournalV4::Undo {
                schema_version: 4,
                request: UndoV4Project {
                    project_id: "source".into(),
                    expected_revision: base.clone(),
                    target_transaction_id: "t1".into(),
                    idempotency_key: "undo".into(),
                },
                changes: vec![],
            }),
            3 => HistoryPackageJournalV1::Causal(CausalJournalV4::Redo {
                schema_version: 4,
                request: RedoV4Project {
                    project_id: "source".into(),
                    expected_revision: base.clone(),
                    undo_transaction_id: "t2".into(),
                    idempotency_key: "redo".into(),
                },
                target_transaction_id: "t1".into(),
                changes: vec![],
            }),
            4 => HistoryPackageJournalV1::Restore(RestoreV4Revision {
                project_id: "source".into(),
                expected_revision: base.clone(),
                target_revision: "r1".into(),
                idempotency_key: "restore".into(),
                asset_versions: Default::default(),
            }),
            _ => HistoryPackageJournalV1::Apply(ApplyV4Project {
                project_id: "source".into(),
                expected_revision: base.clone(),
                idempotency_key: "edit".into(),
                label: "source r0 t1".into(),
                operations: vec![ProjectOperationV4::Rename { name: "r0".into() }],
            }),
        };
        a.revisions.push(HistoryRevisionV1 {
            revision: revision.clone(),
            parent_revision: Some(base.clone()),
            transaction_id: Some(transaction.clone()),
            document,
            created_at: "2026-01-01 00:00:00".into(),
        });
        a.transactions.push(HistoryTransactionV1 {
            transaction_id: transaction.clone(),
            base_revision: base.clone(),
            revision: revision.clone(),
            journal,
            result: TransactionResult {
                transaction_id: transaction,
                base_revision: base,
                revision,
                created_ids: vec![],
                changed_ids: vec!["source".into()],
                deleted_ids: vec![],
            },
            created_at: "2026-01-01 00:00:00".into(),
        });
    }
    HistoryPackageSourceV1 {
        archive: a,
        derived_fonts: vec![],
    }
}
fn prepared(s: &HistoryPackageSourceV1) -> PreparedHistoryImportV1 {
    let manifest = make_manifest(s).unwrap();
    PreparedHistoryImportV1 {
        manifest_sha256: digest(&canonical_bytes(&manifest, MAX_MANIFEST_BYTES).unwrap()),
        manifest,
        archive: s.archive.clone(),
    }
}
#[test]
fn deterministic_mapping_preserves_content_and_every_typed_causal_reference() {
    let p = prepared(&source());
    let mapped = prepare_import(&p, "target").unwrap();
    assert_eq!(
        canonical_bytes(&mapped, MAX_HISTORY_BYTES).unwrap(),
        canonical_bytes(&prepare_import(&p, "target").unwrap(), MAX_HISTORY_BYTES).unwrap()
    );
    assert_ne!(
        mapped.main_head_revision,
        prepare_import(&p, "other").unwrap().main_head_revision
    );
    let edit = mapped
        .transactions
        .iter()
        .find(|t| matches!(t.journal, HistoryPackageJournalV1::Apply(_)))
        .unwrap();
    let HistoryPackageJournalV1::Apply(r) = &edit.journal else {
        unreachable!()
    };
    assert_eq!(r.project_id, "target");
    assert_eq!(r.label, "source r0 t1");
    assert!(matches!(&r.operations[0],ProjectOperationV4::Rename{name} if name=="r0"));
    assert_eq!(edit.result.changed_ids, vec!["target"]);
    let undo = mapped
        .transactions
        .iter()
        .find(|t| {
            matches!(
                t.journal,
                HistoryPackageJournalV1::Causal(CausalJournalV4::Undo { .. })
            )
        })
        .unwrap();
    let HistoryPackageJournalV1::Causal(CausalJournalV4::Undo { request, .. }) = &undo.journal
    else {
        unreachable!()
    };
    assert_eq!(request.target_transaction_id, edit.transaction_id);
    let redo = mapped
        .transactions
        .iter()
        .find(|t| {
            matches!(
                t.journal,
                HistoryPackageJournalV1::Causal(CausalJournalV4::Redo { .. })
            )
        })
        .unwrap();
    let HistoryPackageJournalV1::Causal(CausalJournalV4::Redo {
        request,
        target_transaction_id,
        ..
    }) = &redo.journal
    else {
        unreachable!()
    };
    assert_eq!(request.undo_transaction_id, undo.transaction_id);
    assert_eq!(target_transaction_id, &edit.transaction_id);
    assert_eq!(mapped.branches[0].head_revision, edit.revision);
    assert_eq!(mapped.merges[0].source_revision, edit.revision);
    assert!(
        mapped
            .revisions
            .iter()
            .all(|r| r.document.id == "target"
                && ["source", "r0"].contains(&r.document.name.as_str()))
    );
}
#[test]
fn malformed_closure_causal_edges_and_digest_are_rejected_without_dropping_rows() {
    let original = source();
    validate_source(&original).unwrap();
    let mut bad = original.clone();
    bad.archive.revisions.remove(1);
    assert!(validate_source(&bad).is_err());
    let mut bad = original.clone();
    bad.archive.merges[0].source_revision = "r4".into();
    assert!(validate_source(&bad).is_err());
    let mut bad = original.clone();
    bad.archive.transactions[0].journal = HistoryPackageJournalV1::Restore(RestoreV4Revision {
        project_id: "source".into(),
        expected_revision: "r0".into(),
        target_revision: "r4".into(),
        idempotency_key: "impossible-future".into(),
        asset_versions: Default::default(),
    });
    assert!(validate_source(&bad).is_err());
    let mut bad = original.clone();
    bad.archive.transactions[2]
        .result
        .changed_ids
        .push("ordinary-content".into());
    let mut p = prepared(&bad);
    p.archive.transactions[0].result.revision = "r3".into();
    assert!(validate_prepared(&p).is_err());
    let mut bad = original.clone();
    bad.archive.branches[0].created_from_revision = "outside".into();
    assert!(validate_source(&bad).is_err());
    let mut bad = original.clone();
    if let HistoryPackageJournalV1::Causal(CausalJournalV4::Redo { request, .. }) =
        &mut bad.archive.transactions[2].journal
    {
        request.undo_transaction_id = "t1".into();
    }
    assert!(validate_source(&bad).is_err());
    assert_eq!(
        HistoryPackageJournalV1::from_storage(r#"{"kind":"future_edit","project_id":"source"}"#)
            .unwrap_err()
            .code,
        "PACKAGE_HISTORY_UNSUPPORTED"
    );
}
#[test]
fn public_resource_selector_revisions_are_mapped_but_provenance_ids_are_not() {
    use opencut_contracts::project_v4_public::*;
    let mut s = source();
    s.archive.transactions[0].journal =
        HistoryPackageJournalV1::ApplyPublic(ApplyPublicV4Project {
            project_id: "source".into(),
            expected_revision: "r0".into(),
            idempotency_key: "original-key".into(),
            label: "source".into(),
            operations: vec![
                PublicProjectOperationV4::AssetBind {
                    id: "asset".into(),
                    name: "r0".into(),
                    source: PublicAssetSelectorV4::ProjectAsset {
                        revision: "r0".into(),
                        asset_id: "r0".into(),
                    },
                },
                PublicProjectOperationV4::FontBind {
                    id: "font".into(),
                    source: PublicFontSelectorV4::ProjectFont {
                        revision: "r0".into(),
                        font_id: "t1".into(),
                    },
                },
            ],
        });
    let mapped = prepare_import(&prepared(&s), "target").unwrap();
    let initial = &mapped
        .revisions
        .iter()
        .find(|r| r.parent_revision.is_none())
        .unwrap()
        .revision;
    let edit = mapped
        .transactions
        .iter()
        .find(|t| matches!(t.journal, HistoryPackageJournalV1::ApplyPublic(_)))
        .unwrap();
    let HistoryPackageJournalV1::ApplyPublic(r) = &edit.journal else {
        unreachable!()
    };
    assert!(
        matches!(&r.operations[0],PublicProjectOperationV4::AssetBind{name,source:PublicAssetSelectorV4::ProjectAsset{revision,asset_id},..} if name=="r0" && revision==initial && asset_id=="r0")
    );
    assert!(
        matches!(&r.operations[1],PublicProjectOperationV4::FontBind{source:PublicFontSelectorV4::ProjectFont{revision,font_id},..} if revision==initial && font_id=="t1")
    );
    assert_eq!(r.idempotency_key, "original-key");
}
#[test]
fn known_font_caption_and_catalog_journals_roundtrip_all_original_fields() {
    use opencut_contracts::{
        catalog_resources_v1::*, font_import_v1::ImportFontV1, subtitle_v4::*,
    };
    let style = CaptionStyleV4 {
        font: FontBindingV4::Locked {
            font_id: "font".into(),
        },
        font_size: FontSizeV4::CanvasPixels { value: 24.0 },
        color: ColorV4 {
            encoding: ColorEncodingV4::Srgb,
            red: 1.0,
            green: 1.0,
            blue: 1.0,
            alpha: 1.0,
        },
        background_color: None,
        max_width_px: None,
        line_height_px: None,
        text_align: TextAlignV4::Center,
        layout: CaptionLayoutProfileV4::PinnedCaptionV4,
    };
    let journals = vec![
        HistoryPackageJournalV1::FontImport(ImportFontV1 {
            project_id: "source".into(),
            expected_revision: "r0".into(),
            idempotency_key: "f".into(),
            catalog_id: "pinned".into(),
            font_id: "font".into(),
            required_text: "source r0".into(),
        }),
        HistoryPackageJournalV1::CaptionGenerate(GenerateCaptionsV4 {
            project_id: "source".into(),
            expected_revision: "r0".into(),
            sequence_id: "scene".into(),
            track_id: "captions".into(),
            analysis_job_id: "original-analysis-fact".into(),
            idempotency_key: "cg".into(),
            words_per_caption: 5,
            style: style.clone(),
        }),
        HistoryPackageJournalV1::CaptionImport(SubtitleImportV4 {
            project_id: "source".into(),
            expected_revision: "r0".into(),
            sequence_id: "scene".into(),
            track_id: "captions".into(),
            idempotency_key: "ci".into(),
            format: SubtitleFormatV4::Srt,
            content: "source r0".into(),
            style,
        }),
        HistoryPackageJournalV1::CatalogImport(ImportCatalogResourceV1 {
            project_id: "source".into(),
            expected_revision: "r0".into(),
            idempotency_key: "i".into(),
            catalog_id: "kenney-ui-click1".into(),
            asset_id: "sound".into(),
            uploaded: None,
        }),
        HistoryPackageJournalV1::CatalogGenerate(GenerateCatalogSoundV1 {
            project_id: "source".into(),
            expected_revision: "r0".into(),
            idempotency_key: "g".into(),
            asset_id: "sound".into(),
            recipe: LocalSoundRecipeV1 {
                kind: LocalSoundKindV1::Whoosh,
                duration_ms: 300,
                seed: 42,
                level_db: -6.0,
            },
        }),
    ];
    for journal in journals {
        let raw = journal.storage_json().unwrap();
        assert_eq!(
            HistoryPackageJournalV1::from_storage(&raw)
                .unwrap()
                .storage_json()
                .unwrap(),
            raw
        );
    }
}

#[test]
fn brand_journal_restores_typed_scope_without_rewriting_catalog_or_upload_identity() {
    use opencut_contracts::{
        AssetVersion, brand_resources_v1::ImportBrandResourceV1,
        catalog_resources_v1::CatalogUploadedMediaV1,
    };
    let original = ImportBrandResourceV1 {
        schema_version: 1,
        project_id: "source".into(),
        expected_revision: "r0".into(),
        idempotency_key: "brand-key".into(),
        catalog_id: "simple-icons:16.35.0:dotnet".into(),
        asset_id: "r0".into(),
        uploaded: CatalogUploadedMediaV1 {
            media_id: "source".into(),
            version: AssetVersion {
                id: format!("av_{}", "a".repeat(64)),
                content_hash: "a".repeat(64),
                size: 1024,
            },
        },
    };
    let raw = serde_json::to_string(&original).unwrap();
    let mut journal = HistoryPackageJournalV1::from_storage(&raw).unwrap();
    assert!(matches!(&journal, HistoryPackageJournalV1::BrandImport(_)));
    assert_eq!(journal.storage_json().unwrap(), raw);
    journal
        .remap(
            "target",
            &BTreeMap::from([("r0".into(), "mapped".into())]),
            &BTreeMap::new(),
        )
        .unwrap();
    let HistoryPackageJournalV1::BrandImport(mapped) = journal else {
        unreachable!()
    };
    assert_eq!(mapped.project_id, "target");
    assert_eq!(mapped.expected_revision, "mapped");
    assert_eq!(mapped.asset_id, "r0");
    assert_eq!(mapped.uploaded, original.uploaded);
    assert_eq!(mapped.catalog_id, original.catalog_id);
    assert_eq!(mapped.idempotency_key, original.idempotency_key);
}
