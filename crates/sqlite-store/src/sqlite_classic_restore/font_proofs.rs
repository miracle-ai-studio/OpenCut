//! Adapter semantics only. The ignored fixed-source test uses the existing
//! cache and verifies CAS identity; these tests do not claim browser pixels.
use super::*;
use opencut_api_contracts::classic_restore::ClassicFontMapping;
use opencut_contracts::{font_derived::*, scene::*, scene_ops::*};
use opencut_editor_application::{classic_restore::ClassicResources, font_synthesis as synthesis};
use std::collections::BTreeMap;

fn request(target: &str) -> ImportClassic {
    let source = AnalyzeClassic { target_project_id: target.into(), media_mappings: vec![], font_mappings: vec![],
        content: json!({"version":31,"metadata":{"id":"classic","name":"Classic"},"settings":{"fps":{"numerator":24,"denominator":1},"canvasSize":{"width":64,"height":48},"background":{"type":"color","color":"#000000"}},"scenes":[{"id":"main","name":"Main","isMain":true,"bookmarks":[],"tracks":{"main":{"id":"video","name":"Video","type":"video","elements":[]},"overlay":[],"audio":[]}}]}).to_string() };
    ImportClassic {
        expected_source_content_id: opencut_editor_application::classic_restore::source_content_id(
            &source.content,
        ),
        source,
        idempotency_key: "original-key".into(),
        accepted_loss_codes: vec![],
    }
}
fn empty() -> ClassicResources {
    ClassicResources {
        media: BTreeMap::new(),
        fonts: vec![],
    }
}
fn counts(app: &EditorApplication) -> Vec<i64> {
    [
        "projects",
        "revisions",
        "outbox",
        "audit_records",
        "idempotency",
        "font_derivations",
    ]
    .into_iter()
    .map(|table| {
        app.connection
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    })
    .collect()
}
fn source(app: &mut EditorApplication) -> (FontResource, FontDerivedSource, ImportClassic) {
    let pin = synthesis::source("google-gudea-bold-synthetic-italic-v1").unwrap();
    let font = synthesis::expected_import_resource(&ImportFont {
        project_id: "font-source".into(),
        expected_revision: "fixed".into(),
        idempotency_key: "font-key".into(),
        catalog_id: pin.source.face.id.clone(),
        font_id: "font".into(),
        required_text: "AV".into(),
    })
    .unwrap();
    let mut document = opencut_editor_application::classic_restore::analyze(
        &request("font-source").source,
        &empty(),
    )
    .unwrap()
    .document
    .unwrap();
    document.fonts.push(font.clone());
    app.connection.execute("INSERT INTO projects(project_id,name,head_revision) VALUES('font-source','Source','fixed')", []).unwrap();
    app.connection.execute("INSERT INTO revisions(revision,project_id,document_json) VALUES('fixed','font-source',?1)", [encode_json(&document,"fixture").unwrap()]).unwrap();
    let mut input = request("imported");
    input.source.font_mappings.push(ClassicFontMapping {
        classic_family: font.family.clone(),
        font_weight: FontWeight::Bold,
        font_style: FontStyle::Italic,
        source_project_id: "font-source".into(),
        source_revision: "fixed".into(),
        font_id: font.id.clone(),
    });
    (font, pin.source, input)
}
#[test]
fn canonical_receipt_and_original_historical_requests_replay_without_public_alternatives() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let input = request("restored");
    assert!(
        app.classic_restore_analyze(&input.source)
            .unwrap()
            .blockers
            .is_empty()
    );
    let receipt = app.classic_restore_import(&input).unwrap();
    let before = counts(&app);
    assert_eq!(
        serde_json::to_value(app.classic_restore_import(&input).unwrap()).unwrap(),
        serde_json::to_value(&receipt).unwrap()
    );
    assert_eq!(counts(&app), before);
    let mut changed = input.clone();
    changed.source.target_project_id = "another".into();
    assert_eq!(
        app.classic_restore_import(&changed).unwrap_err().code,
        "IDEMPOTENCY_KEY_REUSED"
    );
    for (family, kind, schema) in [
        ("import_classic_v4", "classic_import_v4", Some(4)),
        ("import_classic_v5", "classic_import_v5", Some(5)),
        ("import_classic", "classic_import", None),
        ("import_classic_recovery_v1", "classic_recovery_v1", None),
    ] {
        let mut original = EditorApplication::open(":memory:").unwrap();
        let hash = if schema.is_some() {
            receipts::identity(&input, family).unwrap()
        } else {
            request_hash(family,"workspace",&json!({"source_content_id":input.expected_source_content_id,"target_project_id":input.source.target_project_id,"media_mappings":[],"accepted_loss_codes":[]})).unwrap()
        };
        let mut raw = json!({"target_project_id":input.source.target_project_id,"revision":"original","source_content_id":input.expected_source_content_id,"asset_count":0,"losses":[]});
        if let Some(schema) = schema {
            raw["schema_version"] = json!(schema);
            raw["font_count"] = json!(0);
            raw["unresolved_fonts"] = json!([]);
        }
        original.connection.execute("INSERT INTO idempotency(operation_family,scope,idempotency_key,request_hash,result_kind,result_json) VALUES(?1,'workspace',?2,?3,?4,?5)",params![family,input.idempotency_key,hash.current_digest(),kind,raw.to_string()]).unwrap();
        let before = counts(&original);
        let replay = original.classic_restore_import(&input).unwrap();
        assert_eq!(replay.revision, "original");
        assert_eq!(replay.font_count, 0);
        assert_eq!(counts(&original), before);
        original
            .connection
            .execute("UPDATE idempotency SET result_kind='unrecognized'", [])
            .unwrap();
        assert_eq!(
            original.classic_restore_import(&input).unwrap_err().code,
            "STORAGE_ERROR"
        );
    }
}
#[test]
fn v5_audit_failure_rolls_back_first_revision_outbox_and_receipt() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    app.connection.execute_batch("CREATE TRIGGER fail_v5 BEFORE INSERT ON audit_records WHEN NEW.action='project.classic_imported' BEGIN SELECT RAISE(ABORT,'forced'); END;").unwrap();
    let input = request("failed");
    let before = counts(&app);
    assert!(app.classic_restore_import(&input).is_err());
    assert_eq!(counts(&app), before);
    app.connection
        .execute_batch("DROP TRIGGER fail_v5")
        .unwrap();
    assert_eq!(
        app.classic_restore_import(&input)
            .unwrap()
            .target_project_id,
        input.source.target_project_id
    );
}
#[test]
fn fixed_synthesis_requires_retained_proof_and_cas_not_provider_string_only() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let (font, proof, input) = source(&mut app);
    assert!(app.classic_restore_analyze(&input.source).is_err());
    sqlite_font_derivations::persist_verified_source(&app.connection, &font, &proof).unwrap();
    let result = app.classic_restore_analyze(&input.source).unwrap();
    assert!(result.blockers.is_empty());
    let before = counts(&app);
    assert!(app.classic_restore_import(&input).is_err()); // No CAS bytes, even for an unused font.
    assert_eq!(counts(&app), before);
    let mut duplicate = input.source.clone();
    duplicate
        .font_mappings
        .push(duplicate.font_mappings[0].clone());
    assert!(app.classic_restore_analyze(&duplicate).is_err());
    let mut wrong = input.source;
    wrong.font_mappings[0].font_style = FontStyle::Normal;
    assert!(app.classic_restore_analyze(&wrong).is_err());
}
#[test]
fn owner_fence_precedes_even_persisted_replay() {
    use opencut_editor_application::{
        access::{Caller, VerifiedCredential},
        audit::{CallSource, ExecutionContext},
    };
    let mut app = EditorApplication::open(":memory:").unwrap();
    let input = request("owner");
    app.classic_restore_import(&input).unwrap();
    app.set_execution_context(ExecutionContext::for_caller(
        CallSource::Http,
        Caller::Credential(VerifiedCredential::from_verified(
            "reader".into(),
            "credential".into(),
        )),
    ));
    let before = counts(&app);
    assert_eq!(
        app.classic_restore_analyze(&input.source).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(
        app.classic_restore_import(&input).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(counts(&app), before);
}
#[test]
fn original_replay_survives_source_resolution_loss_without_new_access_or_proof_reads() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let (_, _, input) = source(&mut app);
    let receipt = ClassicRestoreReceipt {
        target_project_id: input.source.target_project_id.clone(),
        revision: "published".into(),
        source_content_id: input.expected_source_content_id.clone(),
        asset_count: 0,
        font_count: 1,
        unresolved_fonts: vec![],
        losses: vec![],
    };
    let identity = request_hash(
        "import_classic_v5",
        "workspace",
        &serde_json::to_value(&input).unwrap(),
    )
    .unwrap();
    app.connection.execute("INSERT INTO idempotency(operation_family,scope,idempotency_key,request_hash,result_kind,result_json) VALUES('import_classic_v5','workspace',?1,?2,'classic_import_v5',?3)",params![input.idempotency_key,identity.current_digest(),{let mut raw=serde_json::to_value(&receipt).unwrap();raw["schema_version"]=json!(5);raw.to_string()}]).unwrap();
    app.connection
        .execute("DELETE FROM revisions WHERE project_id='font-source'", [])
        .unwrap();
    assert_eq!(
        serde_json::to_value(app.classic_restore_import(&input).unwrap()).unwrap(),
        serde_json::to_value(&receipt).unwrap()
    );
}
fn history_roundtrip(app: &mut EditorApplication, project: &str, head: &str, target: &str) {
    use opencut_api_contracts::project_archive::HistoricalArchiveImport;
    use opencut_editor_application::project_archive as package;
    let captured = app.historical_archive_source(project, head).unwrap();
    // Initial Classic publication is not an editor transaction or an authority
    // grant. Schema 7 needs no made-up import journal/result tag.
    assert!(
        captured
            .archive
            .revisions
            .iter()
            .any(|r| r.parent_revision.is_none() && r.transaction_id.is_none())
    );
    let manifest = package::make_manifest(&captured).unwrap();
    let prepared = package::PreparedHistoricalArchive {
        manifest_sha256: package::digest(
            &package::canonical_bytes(&manifest, package::MAX_MANIFEST_BYTES).unwrap(),
        ),
        manifest,
        archive: captured.archive,
    };
    let request = HistoricalArchiveImport {
        package_path: "/missing-after-publication".into(),
        target_project_id: target.into(),
        expected_manifest_sha256: prepared.manifest_sha256.clone(),
        idempotency_key: format!("restore-{target}"),
    };
    let receipt = app
        .historical_archive_fixture_commit(&request, &prepared)
        .unwrap();
    let expected = package::prepare_import(&prepared, target).unwrap();
    let actual = app
        .historical_archive_source(target, &receipt.revision)
        .unwrap();
    assert_eq!(
        package::canonical_bytes(&actual.archive, package::MAX_HISTORY_BYTES).unwrap(),
        package::canonical_bytes(&expected, package::MAX_HISTORY_BYTES).unwrap()
    );
    let before = counts(app);
    assert_eq!(
        app.historical_archive_replay(&request)
            .unwrap()
            .unwrap()
            .revision,
        receipt.revision
    );
    assert_eq!(counts(app), before);
}
#[test]
fn v5_first_revision_plus_edit_exports_and_restores_complete_schema7_history() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let imported = app.classic_restore_import(&request("classic-v5")).unwrap();
    let edit = app
        .apply_scene_project(&ApplySceneProject {
            project_id: "classic-v5".into(),
            expected_revision: imported.revision,
            idempotency_key: "rename".into(),
            label: "Rename".into(),
            operations: vec![ProjectOperation::Rename {
                name: "Edited".into(),
            }],
        })
        .unwrap();
    history_roundtrip(&mut app, "classic-v5", &edit.revision, "restored-v5");
}
#[test]
#[ignore = "requires existing OPENCUT_FONT_DERIVE_CACHE with the exact pinned Gudea source"]
fn fixed_gudea_import_proof_history_package_restore_and_reopen() {
    use opencut_asset_store::AssetStore;
    use opencut_contracts::{Asset, AssetKind};
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("workspace.sqlite");
    let mut app = EditorApplication::open(&db).unwrap();
    let (font, proof, mut input) = source(&mut app);
    let pinned = synthesis::pinned_source(&proof.face.id).unwrap();
    let actual = synthesis::actual_source(&pinned).unwrap();
    let prefix = format!(
        "https://raw.githubusercontent.com/google/fonts/{}/",
        actual.source_commit
    );
    let path = std::path::PathBuf::from(
        std::env::var("OPENCUT_FONT_DERIVE_CACHE").expect("existing pinned font cache"),
    )
    .join(actual.source_url.strip_prefix(&prefix).unwrap());
    let pinned = AssetStore::open(app.asset_store_root())
        .unwrap()
        .pin_asset(&Asset {
            id: "font".into(),
            name: "Gudea".into(),
            kind: AssetKind::Image,
            uri: path.to_string_lossy().into_owned(),
            duration: None,
            version: None,
            metadata: Default::default(),
        })
        .unwrap();
    assert_eq!(pinned.version.unwrap(), font.content);
    sqlite_font_derivations::persist_verified_source(&app.connection, &font, &proof).unwrap();
    let mut raw: Value = serde_json::from_str(&input.source.content).unwrap();
    raw["scenes"][0]["tracks"]["overlay"] = json!([{ "id":"text", "name":"Title", "type":"text", "elements":[{
        "id":"title", "name":"Title", "type":"text", "startTime":0,"duration":120000,"trimStart":0,"trimEnd":0,
        "params":{"content":"AV","fontFamily":font.family,"fontWeight":"bold","fontStyle":"italic"},"effects":[],"animations":{}
    }]}]);
    input.source.content = raw.to_string();
    input.expected_source_content_id =
        opencut_editor_application::classic_restore::source_content_id(&input.source.content);
    app.connection.execute_batch("CREATE TRIGGER fail_synthesis_v5 BEFORE INSERT ON audit_records WHEN NEW.action='project.classic_imported' BEGIN SELECT RAISE(ABORT,'forced'); END;").unwrap();
    let before = counts(&app);
    assert!(app.classic_restore_import(&input).is_err());
    assert_eq!(counts(&app), before);
    app.connection
        .execute_batch("DROP TRIGGER fail_synthesis_v5")
        .unwrap();
    let receipt = app.classic_restore_import(&input).unwrap();
    let retained = synthesis::proof::read(
        &app,
        &ReadDerivedFontProof {
            project_id: "imported".into(),
            revision: receipt.revision.clone(),
            font_id: font.id.clone(),
        },
    )
    .unwrap();
    assert_eq!(retained.source.source, proof);
    history_roundtrip(&mut app, "imported", &receipt.revision, "restored");
    let restored = app
        .inspect_scene_project(&InspectProject {
            project_id: "restored".into(),
            revision: None,
        })
        .unwrap();
    let kept = synthesis::proof::read(
        &app,
        &ReadDerivedFontProof {
            project_id: "restored".into(),
            revision: restored.revision,
            font_id: font.id,
        },
    )
    .unwrap();
    assert_eq!(kept.source, retained.source);
    assert_eq!(
        app.connection
            .query_row("SELECT count(*) FROM font_derivations", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    drop(app);
    let mut app = EditorApplication::open(&db).unwrap();
    assert_eq!(
        serde_json::to_value(app.classic_restore_import(&input).unwrap()).unwrap(),
        serde_json::to_value(&receipt).unwrap()
    );
}

#[test]
fn corrupt_retained_proof_and_unacknowledged_losses_leave_no_partial_project() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let (font, proof, input) = source(&mut app);
    // Model a corrupted database record without mutating an immutable row.
    app.connection.execute("INSERT INTO font_derivations(derivation_id,source_json,proof_json,output_sha256,output_size) VALUES(?1,?2,'{}',?3,?4)",params![proof.face.derivation_id,encode_json(&proof,"proof").unwrap(),font.content.content_hash,font.content.size.to_string()]).unwrap();
    let before = counts(&app);
    assert!(app.classic_restore_analyze(&input.source).is_err());
    assert!(app.classic_restore_import(&input).is_err());
    assert_eq!(counts(&app), before);

    let mut loss = request("loss-review");
    let mut raw: Value = serde_json::from_str(&loss.source.content).unwrap();
    raw["timelineViewState"] = json!({"zoom": 2});
    loss.source.content = raw.to_string();
    loss.expected_source_content_id =
        opencut_editor_application::classic_restore::source_content_id(&loss.source.content);
    let report = app.classic_restore_analyze(&loss.source).unwrap();
    assert_eq!(report.losses.len(), 1);
    assert_eq!(report.losses[0].code, "SESSION_STATE");
    assert!(app.classic_restore_import(&loss).is_err());
    assert_eq!(counts(&app), before);
    loss.accepted_loss_codes = vec!["SESSION_STATE".into(), "SESSION_STATE".into()];
    assert!(app.classic_restore_import(&loss).is_err());
    loss.accepted_loss_codes = vec!["SESSION_STATE".into()];
    assert_eq!(
        app.classic_restore_import(&loss).unwrap().target_project_id,
        loss.source.target_project_id
    );
}
