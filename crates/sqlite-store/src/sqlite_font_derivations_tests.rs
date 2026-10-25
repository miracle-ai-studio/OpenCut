use super::*;
use opencut_contracts::edit_recovery::*;
use opencut_contracts::{CreateProject, FrameRate, scene_ops::CreateProjectRequest};
use opencut_editor_application::{
    access::{Caller, PrincipalPolicy, ProjectGrant},
    edit_recovery as recovery,
};
fn fixture() -> (EditorApplication, EditorSnapshot, ImportFont) {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let snapshot = app
        .create_scene_project(&CreateProjectRequest {
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
    let request = ImportFont {
        project_id: snapshot.project.id.clone(),
        expected_revision: snapshot.revision.clone(),
        idempotency_key: "derived-import".into(),
        catalog_id: derived::catalog().unwrap()[0].source.face.id.clone(),
        font_id: "font".into(),
        required_text: "AV".into(),
    };
    (app, snapshot, request)
}
fn stage(request: &ImportFont) -> StageEditRecovery {
    StageEditRecovery {
        context: EditRecoveryContext {
            intent: EditRecoveryIntent::FontImport {
                request: request.clone(),
            },
            sequence_id: Some("main".into()),
        },
    }
}
fn read(request: &ImportFont) -> ReadEditRecovery {
    ReadEditRecovery {
        project_id: request.project_id.clone(),
        recovery_id: None,
    }
}
#[test]
fn derived_font_recovery_uses_the_same_discovery_and_exact_original_slot() {
    let (mut app, _, request) = fixture();
    let staged = app.stage_edit_recovery(&stage(&request)).unwrap();
    assert_eq!(
        app.inspect_edit_recovery(&ReadEditRecovery {
            project_id: request.project_id.clone(),
            recovery_id: None
        })
        .unwrap()
        .record,
        Some(staged.clone())
    );
    assert_eq!(
        recovery::inspect(&app, &read(&request))
            .unwrap()
            .record
            .unwrap(),
        staged
    );
    let mut changed = stage(&request);
    let EditRecoveryIntent::FontImport { request } = &mut changed.context.intent else {
        panic!()
    };
    request.required_text.push('X');
    let changed_project_id = request.project_id.clone();
    assert_eq!(
        app.stage_edit_recovery(&changed).unwrap_err().code,
        "IDEMPOTENCY_KEY_REUSED"
    );
    assert_eq!(
        app.ack_edit_recovery(&AckEditRecovery {
            project_id: changed_project_id,
            recovery_id: staged.recovery_id
        })
        .unwrap_err()
        .code,
        "RECOVERY_UNRESOLVED"
    );
    assert_eq!(
        app.connection
            .query_row::<i64, _, _>("SELECT count(*) FROM edit_recovery", [], |r| r.get(0))
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
    let record = app.stage_edit_recovery(&stage(&request)).unwrap();
    app.set_caller(Caller::LocalOwner);
    app.set_principal_policy(policy("1", vec![ProjectAction::Read]))
        .unwrap();
    app.set_caller(caller);
    assert_eq!(
        app.inspect_edit_recovery(&read(&request)).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(
        app.stage_edit_recovery(&stage(&request)).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(
        app.ack_edit_recovery(&AckEditRecovery {
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
        .push(derived::expected_import_resource(&request).unwrap());
    let tx = app
        .connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    persist_proof(
        &tx,
        SceneCommitIntent::DerivedFontImport(&request),
        &snapshot,
    )
    .unwrap();
    assert_eq!(
        tx.query_row::<i64, _, _>("SELECT count(*) FROM font_derivations", [], |r| r.get(0))
            .unwrap(),
        1
    );
    tx.rollback().unwrap();
    assert_eq!(
        app.connection
            .query_row::<i64, _, _>("SELECT count(*) FROM font_derivations", [], |r| r.get(0))
            .unwrap(),
        0
    );
    // White-box storage fixture: no native parsing or CAS execution is claimed.
    let tx = app
        .connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    persist_proof(
        &tx,
        SceneCommitIntent::DerivedFontImport(&request),
        &snapshot,
    )
    .unwrap();
    persist_proof(
        &tx,
        SceneCommitIntent::DerivedFontImport(&request),
        &snapshot,
    )
    .unwrap();
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
    let query = ReadDerivedFontProof {
        project_id: request.project_id.clone(),
        revision: request.expected_revision.clone(),
        font_id: request.font_id.clone(),
    };
    let result = app.read_derived_font_proof(&query).unwrap();
    assert_eq!(
        result.source.face.derivation_id,
        match &snapshot.project.fonts[0].origin {
            ResourceOrigin::Generated { generation_id, .. } => generation_id.clone(),
            _ => panic!(),
        }
    );
    assert!(
        app.connection
            .execute("UPDATE font_derivations SET proof_json='forged'", [])
            .is_err()
    );
    assert!(
        app.connection
            .execute("DELETE FROM font_derivations", [])
            .is_err()
    );
    let mut wrong = query;
    wrong.font_id = "other".into();
    assert_eq!(
        app.read_derived_font_proof(&wrong).unwrap_err().code,
        "NOT_FOUND"
    );
}

#[test]
#[ignore = "requires pinned Karla output in OPENCUT_FONT_DERIVED_CACHE; never downloads or derives"]
fn derived_font_real_import_original_replay_undo_redo_retains_historical_proof() {
    use opencut_asset_store::font_import::ManagedFontPublisher;
    use opencut_contracts::project_history::{RedoRequest, UndoRequest};
    use opencut_editor_application::{
        font_derived::{DerivedFontSource, PinnedDerivedFont},
        font_import::{
            FontImportCheckpoint, FontImportPolicy, FontInspector, ParsedFontFace,
            catalog::FontFetchPolicy,
        },
        scene,
    };
    use std::{cell::Cell, path::PathBuf};

    struct CachedSource {
        path: PathBuf,
        calls: Cell<u32>,
    }
    impl DerivedFontSource for CachedSource {
        fn obtain(
            &self,
            source: &PinnedDerivedFont,
            policy: &FontFetchPolicy,
            checkpoint: &mut dyn FontImportCheckpoint,
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
    impl FontInspector for HistoryInspector {
        fn inspect(
            &self,
            _: &[u8],
            required_text: &str,
            _: &FontImportPolicy,
            checkpoint: &mut dyn FontImportCheckpoint,
        ) -> Result<ParsedFontFace, EditorError> {
            checkpoint.check()?;
            Ok(ParsedFontFace {
                family: "Karla".into(),
                weight: 400,
                style: FontStyle::Normal,
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
                && pin.source.face.style == FontStyle::Normal
        })
        .expect("fixed Karla regular derivation pin");
    assert_eq!(
        karla.source.face.content_hash,
        "020253eac2f832850ce691eb650c60cae7ce29927b7f7c50bc7975f9376af91a"
    );
    assert_eq!(karla.source.face.size_bytes, "45884");
    let (mut app, original, mut request) = fixture();
    request.catalog_id = karla.source.face.id;
    let publisher = ManagedFontPublisher::new(app.asset_store_root());
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
        scene::import_derived_font(
            app,
            &provider,
            &HistoryInspector,
            &publisher,
            &request,
            &FontFetchPolicy::default(),
            &FontImportPolicy::default(),
            &mut || Ok(()),
        )
    };
    let receipt = import(&mut app).unwrap();
    assert_eq!(provider.calls.get(), 1);
    let imported = app
        .inspect_scene_project(&InspectProject {
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
    assert_eq!(serde_json::from_str::<ImportFont>(&raw).unwrap(), request);
    assert_eq!(import(&mut app).unwrap(), receipt);
    assert_eq!(
        provider.calls.get(),
        1,
        "authorized replay must not acquire font bytes"
    );
    let query = ReadDerivedFontProof {
        project_id: request.project_id.clone(),
        revision: receipt.revision.clone(),
        font_id: request.font_id.clone(),
    };
    let before = app.read_derived_font_proof(&query).unwrap();
    let undo = app
        .apply_project_undo(&UndoRequest {
            project_id: request.project_id.clone(),
            expected_revision: receipt.revision.clone(),
            target_transaction_id: receipt.transaction_id.clone(),
            idempotency_key: "undo-derived-font".into(),
        })
        .unwrap();
    assert_eq!(
        app.inspect_scene_project(&InspectProject {
            project_id: request.project_id.clone(),
            revision: None,
        })
        .unwrap()
        .project,
        original.project
    );
    assert_eq!(app.read_derived_font_proof(&query).unwrap(), before);
    app.apply_project_redo(&RedoRequest {
        project_id: request.project_id.clone(),
        expected_revision: undo.transaction.revision,
        undo_transaction_id: undo.transaction.transaction_id,
        idempotency_key: "redo-derived-font".into(),
    })
    .unwrap();
    assert_eq!(
        app.inspect_scene_project(&InspectProject {
            project_id: request.project_id.clone(),
            revision: None,
        })
        .unwrap()
        .project,
        imported.project
    );
    assert_eq!(app.read_derived_font_proof(&query).unwrap(), before);
    assert_eq!(import(&mut app).unwrap(), receipt);
    assert_eq!(provider.calls.get(), 1);
    app.set_caller(Caller::LocalOwner);
    app.revoke_credential(&issued.credential_id).unwrap();
    app.set_caller(caller);
    assert_eq!(
        app.read_derived_font_proof(&query).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(import(&mut app).unwrap_err().code, "NOT_AUTHORIZED");
    assert_eq!(
        provider.calls.get(),
        1,
        "revoked replay must not call the provider"
    );
}
