use super::*;
use opencut_contracts::{
    CreateProject, FrameRate, font_recovery_v1::ReadFontRecoveryV1, project_v4_ops::CreateV4Project,
};
use opencut_editor_application::{
    access::{Caller, PrincipalPolicy, ProjectGrant},
    font_derived_v1::recovery,
};
fn fixture() -> (EditorApplication, ProjectSnapshotV4, ImportDerivedFontV1) {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let snapshot = app
        .create_v4_project(&CreateV4Project {
            project: CreateProject {
                id: "derived-project".into(),
                name: "Fonts".into(),
                sequence_id: "main".into(),
                width: 64,
                height: 48,
                fps: FrameRate {
                    numerator: 25,
                    denominator: 1,
                },
            },
            idempotency_key: "create-derived-project".into(),
        })
        .unwrap();
    let request = ImportDerivedFontV1 {
        project_id: snapshot.project.id.clone(),
        expected_revision: snapshot.revision.clone(),
        idempotency_key: "derived-import".into(),
        catalog_id: derived::catalog().unwrap()[0].source.face.id.clone(),
        font_id: "font".into(),
        required_text: "AV".into(),
    };
    (app, snapshot, request)
}
fn stage(request: &ImportDerivedFontV1) -> StageDerivedFontRecoveryV1 {
    StageDerivedFontRecoveryV1 {
        context: DerivedFontRecoveryContextV1 {
            request: request.clone(),
            sequence_id: Some("main".into()),
        },
    }
}
fn read(request: &ImportDerivedFontV1) -> ReadDerivedFontRecoveryV1 {
    ReadDerivedFontRecoveryV1 {
        project_id: request.project_id.clone(),
        recovery_id: None,
    }
}
#[test]
fn derived_font_recovery_shares_slot_without_cross_protocol_discovery_or_clear() {
    let (mut app, _, request) = fixture();
    let staged = app
        .stage_derived_font_recovery_v1(&stage(&request))
        .unwrap();
    assert!(
        app.inspect_font_recovery_v1(&ReadFontRecoveryV1 {
            project_id: request.project_id.clone(),
            recovery_id: None
        })
        .unwrap()
        .record
        .is_none()
    );
    assert_eq!(
        recovery::read(&app, &read(&request))
            .unwrap()
            .record
            .unwrap(),
        staged
    );
    let mut changed = stage(&request);
    changed.context.request.required_text.push('X');
    assert_eq!(
        app.stage_derived_font_recovery_v1(&changed)
            .unwrap_err()
            .code,
        "IDEMPOTENCY_KEY_REUSED"
    );
    assert_eq!(
        app.ack_derived_font_recovery_v1(&AckDerivedFontRecoveryV1 {
            project_id: request.project_id.clone(),
            recovery_id: staged.recovery_id
        })
        .unwrap_err()
        .code,
        "RECOVERY_UNRESOLVED"
    );
    assert_eq!(
        app.connection
            .query_row::<i64, _, _>("SELECT count(*) FROM edit_recovery_v2", [], |r| r.get(0))
            .unwrap(),
        1
    );
}
#[test]
fn derived_font_recovery_rechecks_original_edit_authority() {
    let (mut app, _, request) = fixture();
    let policy = |revision: &str, actions| PrincipalPolicy {
        principal_id: "derived-agent".into(),
        expected_revision: revision.into(),
        enabled: true,
        grants: vec![ProjectGrant {
            project_id: request.project_id.clone(),
            actions,
        }],
    };
    app.set_principal_policy(policy("0", vec![ProjectAction::Read, ProjectAction::Edit]))
        .unwrap();
    let credential = app.issue_credential("derived-agent").unwrap();
    let caller = app.authenticate(&credential.secret).unwrap();
    app.set_caller(caller.clone());
    let record = app
        .stage_derived_font_recovery_v1(&stage(&request))
        .unwrap();
    app.set_caller(Caller::LocalOwner);
    app.set_principal_policy(policy("1", vec![ProjectAction::Read]))
        .unwrap();
    app.set_caller(caller);
    assert_eq!(
        app.inspect_derived_font_recovery_v1(&read(&request))
            .unwrap_err()
            .code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(
        app.stage_derived_font_recovery_v1(&stage(&request))
            .unwrap_err()
            .code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(
        app.ack_derived_font_recovery_v1(&AckDerivedFontRecoveryV1 {
            project_id: request.project_id,
            recovery_id: record.recovery_id
        })
        .unwrap_err()
        .code,
        "NOT_AUTHORIZED"
    );
}
#[test]
fn derived_font_proof_is_immutable_transactional_and_bound_to_fixed_revision() {
    let (mut app, mut snapshot, request) = fixture();
    snapshot
        .project
        .fonts
        .push(derived::expected_import_resource_v1(&request).unwrap());
    let tx = app
        .connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    persist_proof(&tx, V4Intent::DerivedFontImport(&request), &snapshot).unwrap();
    assert_eq!(
        tx.query_row::<i64, _, _>("SELECT count(*) FROM font_derivations_v1", [], |r| r.get(0))
            .unwrap(),
        1
    );
    tx.rollback().unwrap();
    assert_eq!(
        app.connection
            .query_row::<i64, _, _>("SELECT count(*) FROM font_derivations_v1", [], |r| r.get(0))
            .unwrap(),
        0
    );
    // White-box storage fixture: no native parsing or CAS execution is claimed.
    let tx = app
        .connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    persist_proof(&tx, V4Intent::DerivedFontImport(&request), &snapshot).unwrap();
    persist_proof(&tx, V4Intent::DerivedFontImport(&request), &snapshot).unwrap();
    tx.execute(
        "UPDATE revisions SET document_json=?1 WHERE project_id=?2 AND revision=?3",
        params![
            serde_json::to_string(&snapshot.project).unwrap(),
            request.project_id,
            request.expected_revision
        ],
    )
    .unwrap();
    tx.commit().unwrap();
    let query = ReadDerivedFontProofV1 {
        project_id: request.project_id.clone(),
        revision: request.expected_revision.clone(),
        font_id: request.font_id.clone(),
    };
    let result = app.read_derived_font_proof_v1(&query).unwrap();
    assert_eq!(
        result.source.face.derivation_id,
        match &snapshot.project.fonts[0].origin {
            ResourceOriginV4::Generated { generation_id, .. } => generation_id.clone(),
            _ => panic!(),
        }
    );
    assert!(
        app.connection
            .execute("UPDATE font_derivations_v1 SET proof_json='forged'", [])
            .is_err()
    );
    assert!(
        app.connection
            .execute("DELETE FROM font_derivations_v1", [])
            .is_err()
    );
    let mut wrong = query;
    wrong.font_id = "other".into();
    assert_eq!(
        app.read_derived_font_proof_v1(&wrong).unwrap_err().code,
        "NOT_FOUND"
    );
}

#[test]
#[ignore = "requires pinned Karla output in OPENCUT_FONT_DERIVED_CACHE; never downloads or derives"]
fn derived_font_real_import_original_replay_undo_redo_retains_historical_proof() {
    use opencut_asset_store::font_import_v1::ManagedFontPublisherV1;
    use opencut_contracts::project_v4_history::{RedoV4Project, UndoV4Project};
    use opencut_editor_application::{
        font_derived_v1::{DerivedFontSourceV1, PinnedDerivedFontV1},
        font_import_v1::{
            FontImportCheckpointV1, FontImportPolicyV1, FontInspectorV1, ParsedFontFaceV1,
            catalog::FontFetchPolicyV1,
        },
        project_v4,
    };
    use std::{cell::Cell, path::PathBuf};

    struct CachedSource {
        path: PathBuf,
        calls: Cell<u32>,
    }
    impl DerivedFontSourceV1 for CachedSource {
        fn obtain(
            &self,
            source: &PinnedDerivedFontV1,
            policy: &FontFetchPolicyV1,
            checkpoint: &mut dyn FontImportCheckpointV1,
        ) -> Result<Vec<u8>, EditorError> {
            checkpoint.check()?;
            assert!(!policy.allow_public_font_download);
            assert_eq!(source.source.face.family, "Karla");
            self.calls.set(self.calls.get() + 1);
            std::fs::read(&self.path)
                .map_err(|error| EditorError::new("FONT_FIXTURE_UNAVAILABLE", error.to_string()))
        }
    }
    // Only the history/atomic-store path is under test here. The fixed bytes'
    // real native face inspection and public online/offline loading are covered
    // separately; this mock does not claim glyph or native parser validation.
    struct HistoryInspector;
    impl FontInspectorV1 for HistoryInspector {
        fn inspect(
            &self,
            _: &[u8],
            required_text: &str,
            _: &FontImportPolicyV1,
            checkpoint: &mut dyn FontImportCheckpointV1,
        ) -> Result<ParsedFontFaceV1, EditorError> {
            checkpoint.check()?;
            Ok(ParsedFontFaceV1 {
                family: "Karla".into(),
                weight: 400,
                style: FontStyleV4::Normal,
                glyph_count: 1,
                checked_codepoints: required_text.chars().count() as u32,
            })
        }
    }
    let cache = PathBuf::from(
        std::env::var_os("OPENCUT_FONT_DERIVED_CACHE")
            .expect("explicit existing derived font cache required"),
    );
    let karla = derived::catalog()
        .unwrap()
        .into_iter()
        .find(|pin| {
            pin.source.face.family == "Karla"
                && pin.source.face.weight == 400
                && pin.source.face.style == FontStyleV4::Normal
        })
        .expect("fixed Karla regular derivation pin");
    assert_eq!(
        karla.source.face.content_hash,
        "020253eac2f832850ce691eb650c60cae7ce29927b7f7c50bc7975f9376af91a"
    );
    assert_eq!(karla.source.face.size_bytes, "45884");
    let (mut app, original, mut request) = fixture();
    request.catalog_id = karla.source.face.id;
    let publisher = ManagedFontPublisherV1::new(app.asset_store_root());
    let provider = CachedSource {
        path: cache.join("derived").join(&karla.source.face.content_hash),
        calls: Cell::new(0),
    };
    app.set_principal_policy(PrincipalPolicy {
        principal_id: "derived-history-agent".into(),
        expected_revision: "0".into(),
        enabled: true,
        grants: vec![ProjectGrant {
            project_id: request.project_id.clone(),
            actions: vec![
                ProjectAction::Read,
                ProjectAction::Edit,
                ProjectAction::Restore,
            ],
        }],
    })
    .unwrap();
    let issued = app.issue_credential("derived-history-agent").unwrap();
    let caller = app.authenticate(&issued.secret).unwrap();
    app.set_caller(caller.clone());
    let import = |app: &mut EditorApplication| {
        project_v4::import_derived_font_v1(
            app,
            &provider,
            &HistoryInspector,
            &publisher,
            &request,
            &FontFetchPolicyV1::default(),
            &FontImportPolicyV1::default(),
            &mut || Ok(()),
        )
    };
    let receipt = import(&mut app).unwrap();
    assert_eq!(provider.calls.get(), 1);
    let imported = app
        .inspect_v4_project(&InspectV4Project {
            project_id: request.project_id.clone(),
            revision: Some(receipt.revision.clone()),
        })
        .unwrap();
    let raw: String = app
        .connection
        .query_row(
            "SELECT request_json FROM editor_transactions WHERE transaction_id=?1",
            [&receipt.transaction_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<ImportDerivedFontV1>(&raw).unwrap(),
        request
    );
    assert_eq!(import(&mut app).unwrap(), receipt);
    assert_eq!(
        provider.calls.get(),
        1,
        "authorized replay must not acquire font bytes"
    );
    let query = ReadDerivedFontProofV1 {
        project_id: request.project_id.clone(),
        revision: receipt.revision.clone(),
        font_id: request.font_id.clone(),
    };
    let before = app.read_derived_font_proof_v1(&query).unwrap();
    let undo = app
        .apply_v4_undo(&UndoV4Project {
            project_id: request.project_id.clone(),
            expected_revision: receipt.revision.clone(),
            target_transaction_id: receipt.transaction_id.clone(),
            idempotency_key: "undo-derived-font".into(),
        })
        .unwrap();
    assert_eq!(
        app.inspect_v4_project(&InspectV4Project {
            project_id: request.project_id.clone(),
            revision: None,
        })
        .unwrap()
        .project,
        original.project
    );
    assert_eq!(app.read_derived_font_proof_v1(&query).unwrap(), before);
    app.apply_v4_redo(&RedoV4Project {
        project_id: request.project_id.clone(),
        expected_revision: undo.transaction.revision,
        undo_transaction_id: undo.transaction.transaction_id,
        idempotency_key: "redo-derived-font".into(),
    })
    .unwrap();
    assert_eq!(
        app.inspect_v4_project(&InspectV4Project {
            project_id: request.project_id.clone(),
            revision: None,
        })
        .unwrap()
        .project,
        imported.project
    );
    assert_eq!(app.read_derived_font_proof_v1(&query).unwrap(), before);
    assert_eq!(import(&mut app).unwrap(), receipt);
    assert_eq!(provider.calls.get(), 1);
    app.set_caller(Caller::LocalOwner);
    app.revoke_credential(&issued.credential_id).unwrap();
    app.set_caller(caller);
    assert_eq!(
        app.read_derived_font_proof_v1(&query).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(import(&mut app).unwrap_err().code, "NOT_AUTHORIZED");
    assert_eq!(
        provider.calls.get(),
        1,
        "revoked replay must not call the provider"
    );
}
