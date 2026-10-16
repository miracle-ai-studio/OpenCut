//! Adapter semantics only. The ignored fixed-source test uses the existing
//! cache and verifies CAS identity; these tests do not claim browser pixels.
use super::*;
use opencut_api_contracts::classic_import_v4::ClassicFontMappingV4;
use opencut_contracts::{font_derived_v1::*, project_v4::*, project_v4_ops::*};
use opencut_editor_application::{
    classic_import_v4::ClassicImportResourcesV4, font_synthesis_v1 as synthesis,
};
use std::collections::BTreeMap;

fn request(target: &str) -> ClassicImportV5 {
    let source = ClassicAnalyzeV5 { target_project_id: target.into(), media_mappings: vec![], font_mappings: vec![],
        content: json!({"version":31,"metadata":{"id":"classic","name":"Classic"},"settings":{"fps":{"numerator":24,"denominator":1},"canvasSize":{"width":64,"height":48},"background":{"type":"color","color":"#000000"}},"scenes":[{"id":"main","name":"Main","isMain":true,"bookmarks":[],"tracks":{"main":{"id":"video","name":"Video","type":"video","elements":[]},"overlay":[],"audio":[]}}]}).to_string() };
    ClassicImportV5 {
        expected_source_content_id: opencut_editor_application::classic_import::source_content_id(
            &source.content,
        ),
        source,
        idempotency_key: "original-key".into(),
        accepted_loss_codes: vec![],
    }
}
fn empty() -> ClassicImportResourcesV4 {
    ClassicImportResourcesV4 {
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
        "font_derivations_v1",
    ]
    .into_iter()
    .map(|table| {
        app.connection
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    })
    .collect()
}
fn source(app: &mut EditorApplication) -> (FontResourceV4, FontDerivedSourceV1, ClassicImportV5) {
    let pin = synthesis::source("google-gudea-bold-synthetic-italic-v1").unwrap();
    let font = synthesis::expected_import_resource(&ImportDerivedFontV1 {
        project_id: "font-source".into(),
        expected_revision: "fixed".into(),
        idempotency_key: "font-key".into(),
        catalog_id: pin.source.face.id.clone(),
        font_id: "font".into(),
        required_text: "AV".into(),
    })
    .unwrap();
    let mut document = opencut_editor_application::classic_import_v5::analyze(
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
    input.source.font_mappings.push(ClassicFontMappingV4 {
        classic_family: font.family.clone(),
        font_weight: FontWeightV4::Bold,
        font_style: FontStyleV4::Italic,
        source_project_id: "font-source".into(),
        source_revision: "fixed".into(),
        font_id: font.id.clone(),
    });
    (font, pin.source, input)
}
#[test]
fn versioned_receipt_replay_and_request_family_are_independent_of_v4() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let input = request("new-v5");
    assert_eq!(
        app.classic_v5_analyze(&input.source)
            .unwrap()
            .schema_version,
        5
    );
    let first = app.classic_v5_import(&input).unwrap();
    assert_eq!(first.schema_version, 5);
    let before = counts(&app);
    assert_eq!(
        serde_json::to_value(app.classic_v5_import(&input).unwrap()).unwrap(),
        serde_json::to_value(&first).unwrap()
    );
    assert_eq!(counts(&app), before);
    let old = request("old-v4");
    assert_eq!(app.classic_v4_import(&old).unwrap().schema_version, 4);
    let mut conflict = input.clone();
    conflict.source.target_project_id = "other".into();
    assert_eq!(
        app.classic_v5_import(&conflict).unwrap_err().code,
        "IDEMPOTENCY_KEY_REUSED"
    );
    let mut wrong_hash = input;
    wrong_hash.expected_source_content_id = "forged".into();
    assert!(app.classic_v5_import(&wrong_hash).is_err());
    let rows: Vec<(String, String)> = app
        .connection
        .prepare("SELECT operation_family,result_kind FROM idempotency ORDER BY operation_family")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        rows,
        vec![
            ("import_classic_v4".into(), "classic_import_v4".into()),
            ("import_classic_v5".into(), "classic_import_v5".into())
        ]
    );
}
#[test]
fn v5_audit_failure_rolls_back_first_revision_outbox_and_receipt() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    app.connection.execute_batch("CREATE TRIGGER fail_v5 BEFORE INSERT ON audit_records WHEN NEW.action='project.classic_imported_v5' BEGIN SELECT RAISE(ABORT,'forced'); END;").unwrap();
    let input = request("failed");
    let before = counts(&app);
    assert!(app.classic_v5_import(&input).is_err());
    assert_eq!(counts(&app), before);
    app.connection
        .execute_batch("DROP TRIGGER fail_v5")
        .unwrap();
    assert_eq!(app.classic_v5_import(&input).unwrap().schema_version, 5);
}
#[test]
fn fixed_synthesis_requires_retained_proof_and_cas_not_provider_string_only() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let (font, proof, input) = source(&mut app);
    assert!(app.classic_v5_analyze(&input.source).is_err());
    sqlite_font_derived_v1::persist_verified_source(&app.connection, &font, &proof).unwrap();
    assert!(app.classic_v4_analyze(&input.source).is_err());
    let result = app.classic_v5_analyze(&input.source).unwrap();
    assert!(result.blockers.is_empty());
    let before = counts(&app);
    assert!(app.classic_v5_import(&input).is_err()); // No CAS bytes, even for an unused font.
    assert_eq!(counts(&app), before);
    let mut duplicate = input.source.clone();
    duplicate
        .font_mappings
        .push(duplicate.font_mappings[0].clone());
    assert!(app.classic_v5_analyze(&duplicate).is_err());
    let mut wrong = input.source;
    wrong.font_mappings[0].font_style = FontStyleV4::Normal;
    assert!(app.classic_v5_analyze(&wrong).is_err());
}
#[test]
fn owner_fence_precedes_even_persisted_v5_replay() {
    use opencut_editor_application::{
        access::{Caller, VerifiedCredential},
        audit::{CallSource, ExecutionContext},
    };
    let mut app = EditorApplication::open(":memory:").unwrap();
    let input = request("owner");
    app.classic_v5_import(&input).unwrap();
    app.set_execution_context(ExecutionContext::for_caller(
        CallSource::Http,
        Caller::Credential(VerifiedCredential::from_verified(
            "reader".into(),
            "credential".into(),
        )),
    ));
    let before = counts(&app);
    assert_eq!(
        app.classic_v5_analyze(&input.source).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(
        app.classic_v5_import(&input).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(counts(&app), before);
}
#[test]
fn original_replay_survives_source_resolution_loss_without_new_access_or_proof_reads() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let (_, _, input) = source(&mut app);
    let receipt = ClassicImportReceiptV5 {
        schema_version: 5,
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
    app.connection.execute("INSERT INTO idempotency(operation_family,scope,idempotency_key,request_hash,result_kind,result_json) VALUES('import_classic_v5','workspace',?1,?2,'classic_import_v5',?3)",params![input.idempotency_key,identity.current_digest(),encode_json(&receipt,"receipt").unwrap()]).unwrap();
    app.connection
        .execute("DELETE FROM revisions WHERE project_id='font-source'", [])
        .unwrap();
    assert_eq!(
        serde_json::to_value(app.classic_v5_import(&input).unwrap()).unwrap(),
        serde_json::to_value(&receipt).unwrap()
    );
}
fn history_roundtrip(app: &mut EditorApplication, project: &str, head: &str, target: &str) {
    use opencut_api_contracts::project_history_package_v1::ImportHistoryPackageV1;
    use opencut_editor_application::project_history_package_v1 as package;
    let captured = app.history_package_v1_export_source(project, head).unwrap();
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
    let prepared = package::PreparedHistoryImportV1 {
        manifest_sha256: package::digest(
            &package::canonical_bytes(&manifest, package::MAX_MANIFEST_BYTES).unwrap(),
        ),
        manifest,
        archive: captured.archive,
    };
    let request = ImportHistoryPackageV1 {
        package_path: "/missing-after-publication".into(),
        target_project_id: target.into(),
        expected_manifest_sha256: prepared.manifest_sha256.clone(),
        idempotency_key: format!("restore-{target}"),
    };
    let receipt = app
        .history_package_v1_import_commit(&request, &prepared)
        .unwrap();
    let expected = package::prepare_import(&prepared, target).unwrap();
    let actual = app
        .history_package_v1_export_source(target, &receipt.revision)
        .unwrap();
    assert_eq!(
        package::canonical_bytes(&actual.archive, package::MAX_HISTORY_BYTES).unwrap(),
        package::canonical_bytes(&expected, package::MAX_HISTORY_BYTES).unwrap()
    );
    let before = counts(app);
    assert_eq!(
        app.history_package_v1_import_replay(&request)
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
    let imported = app.classic_v5_import(&request("classic-v5")).unwrap();
    let edit = app
        .apply_v4_project(&ApplyV4Project {
            project_id: "classic-v5".into(),
            expected_revision: imported.revision,
            idempotency_key: "rename".into(),
            label: "Rename".into(),
            operations: vec![ProjectOperationV4::Rename {
                name: "Edited".into(),
            }],
        })
        .unwrap();
    history_roundtrip(&mut app, "classic-v5", &edit.revision, "restored-v5");
}
#[test]
#[ignore = "requires existing OPENCUT_FONT_DERIVE_CACHE with the exact pinned Gudea source"]
fn fixed_gudea_v5_import_proof_history_package_restore_and_reopen() {
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
    sqlite_font_derived_v1::persist_verified_source(&app.connection, &font, &proof).unwrap();
    let mut raw: Value = serde_json::from_str(&input.source.content).unwrap();
    raw["scenes"][0]["tracks"]["overlay"] = json!([{ "id":"text", "name":"Title", "type":"text", "elements":[{
        "id":"title", "name":"Title", "type":"text", "startTime":0,"duration":120000,"trimStart":0,"trimEnd":0,
        "params":{"content":"AV","fontFamily":font.family,"fontWeight":"bold","fontStyle":"italic"},"effects":[],"animations":{}
    }]}]);
    input.source.content = raw.to_string();
    input.expected_source_content_id =
        opencut_editor_application::classic_import::source_content_id(&input.source.content);
    app.connection.execute_batch("CREATE TRIGGER fail_synthesis_v5 BEFORE INSERT ON audit_records WHEN NEW.action='project.classic_imported_v5' BEGIN SELECT RAISE(ABORT,'forced'); END;").unwrap();
    let before = counts(&app);
    assert!(app.classic_v5_import(&input).is_err());
    assert_eq!(counts(&app), before);
    app.connection
        .execute_batch("DROP TRIGGER fail_synthesis_v5")
        .unwrap();
    let receipt = app.classic_v5_import(&input).unwrap();
    let retained = synthesis::proof::read(
        &app,
        &ReadDerivedFontProofV1 {
            project_id: "imported".into(),
            revision: receipt.revision.clone(),
            font_id: font.id.clone(),
        },
    )
    .unwrap();
    assert_eq!(retained.source.source, proof);
    history_roundtrip(&mut app, "imported", &receipt.revision, "restored");
    let restored = app
        .inspect_v4_project(&InspectV4Project {
            project_id: "restored".into(),
            revision: None,
        })
        .unwrap();
    let kept = synthesis::proof::read(
        &app,
        &ReadDerivedFontProofV1 {
            project_id: "restored".into(),
            revision: restored.revision,
            font_id: font.id,
        },
    )
    .unwrap();
    assert_eq!(kept.source, retained.source);
    assert_eq!(
        app.connection
            .query_row("SELECT count(*) FROM font_derivations_v1", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    drop(app);
    let mut app = EditorApplication::open(&db).unwrap();
    assert_eq!(
        serde_json::to_value(app.classic_v5_import(&input).unwrap()).unwrap(),
        serde_json::to_value(&receipt).unwrap()
    );
}

#[test]
fn corrupt_retained_proof_and_unacknowledged_losses_leave_no_partial_project() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let (font, proof, input) = source(&mut app);
    // Model a corrupted database record without mutating an immutable row.
    app.connection.execute("INSERT INTO font_derivations_v1(derivation_id,source_json,proof_json,output_sha256,output_size) VALUES(?1,?2,'{}',?3,?4)",params![proof.face.derivation_id,encode_json(&proof,"proof").unwrap(),font.content.content_hash,font.content.size.to_string()]).unwrap();
    let before = counts(&app);
    assert!(app.classic_v5_analyze(&input.source).is_err());
    assert!(app.classic_v5_import(&input).is_err());
    assert_eq!(counts(&app), before);

    let mut loss = request("loss-review");
    let mut raw: Value = serde_json::from_str(&loss.source.content).unwrap();
    raw["timelineViewState"] = json!({"zoom": 2});
    loss.source.content = raw.to_string();
    loss.expected_source_content_id =
        opencut_editor_application::classic_import::source_content_id(&loss.source.content);
    let report = app.classic_v5_analyze(&loss.source).unwrap();
    assert_eq!(report.losses.len(), 1);
    assert_eq!(report.losses[0].code, "SESSION_STATE");
    assert!(app.classic_v5_import(&loss).is_err());
    assert_eq!(counts(&app), before);
    loss.accepted_loss_codes = vec!["SESSION_STATE".into(), "SESSION_STATE".into()];
    assert!(app.classic_v5_import(&loss).is_err());
    loss.accepted_loss_codes = vec!["SESSION_STATE".into()];
    assert_eq!(app.classic_v5_import(&loss).unwrap().schema_version, 5);
}
