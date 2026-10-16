//! White-box persistence/recovery fixtures. No font bytes, native parsing or
//! browser synthesis execution is claimed by these tests.
use super::*;
use opencut_contracts::{CreateProject, FrameRate};
use opencut_editor_application::font_synthesis_v1 as synthesis;
fn fixture() -> (EditorApplication, ProjectSnapshotV4, ImportDerivedFontV1) {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let snapshot = app
        .create_v4_project(&CreateV4Project {
            project: CreateProject {
                id: "synthesis-project".into(),
                name: "Fonts".into(),
                sequence_id: "main".into(),
                width: 64,
                height: 48,
                fps: FrameRate {
                    numerator: 25,
                    denominator: 1,
                },
            },
            idempotency_key: "create-synthesis".into(),
        })
        .unwrap();
    let request = ImportDerivedFontV1 {
        project_id: snapshot.project.id.clone(),
        expected_revision: snapshot.revision.clone(),
        idempotency_key: "original-synthesis-import".into(),
        catalog_id: synthesis::catalog().unwrap().remove(0).source.face.id,
        font_id: "synthesis-font".into(),
        required_text: "AV".into(),
    };
    (app, snapshot, request)
}
#[test]
fn synthesis_proof_commits_and_rolls_back_in_original_immutable_table_and_reads_version3() {
    let (mut app, mut snapshot, request) = fixture();
    let source = synthesis::pinned_source(&request.catalog_id)
        .unwrap()
        .source;
    let font = synthesis::expected_import_resource(&request).unwrap();
    snapshot.project.fonts.push(font.clone());
    let tx = app
        .connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    require_derived_proposal(&tx, V4Intent::DerivedFontImport(&request), &snapshot).unwrap();
    persist_proof(&tx, V4Intent::DerivedFontImport(&request), &snapshot).unwrap();
    tx.rollback().unwrap();
    assert_eq!(
        app.connection
            .query_row("SELECT COUNT(*) FROM font_derivations_v1", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        0
    );
    let tx = app
        .connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    persist_verified_source(&tx, &font, &source).unwrap();
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
    let proof = app.read_derived_font_proof_v1(&query).unwrap();
    assert_eq!(proof.schema_version, 3);
    assert_eq!(proof.source, source);
    let typed = synthesis::proof::read(&app, &query).unwrap();
    assert_eq!(typed.source.source, source);
    assert_eq!(typed.source.binding.requested.style, FontStyleV4::Italic);
    assert_eq!(typed.source.binding.actual.style, FontStyleV4::Normal);
    assert!(derived::proof::read(&app, &query).is_err());
    assert!(opencut_editor_application::font_derived_v2::proof::read(&app, &query).is_err());
    assert_eq!(
        app.connection
            .query_row("SELECT COUNT(*) FROM font_derivations_v1", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        1
    );
    assert!(
        app.connection
            .execute("UPDATE font_derivations_v1 SET proof_json='{}'", [])
            .is_err()
    );
    assert!(
        app.connection
            .execute("DELETE FROM font_derivations_v1", [])
            .is_err()
    );
    let mut forged = font;
    let ResourceOriginV4::Generated { provider, .. } = &mut forged.origin else {
        panic!()
    };
    *provider = derived::DERIVED_FONT_PROVIDER_V1.into();
    assert!(read_source_for_font(&app.connection, &forged).is_err());
}
#[test]
fn synthesis_recovery_retains_original_request_without_writing_proof_or_revision() {
    let (mut app, before, request) = fixture();
    let stage = StageDerivedFontRecoveryV1 {
        context: DerivedFontRecoveryContextV1 {
            request: request.clone(),
            sequence_id: Some("main".into()),
        },
    };
    let first = app.stage_derived_font_recovery_v1(&stage).unwrap();
    assert_eq!(app.stage_derived_font_recovery_v1(&stage).unwrap(), first);
    let found = app
        .inspect_derived_font_recovery_v1(&ReadDerivedFontRecoveryV1 {
            project_id: request.project_id.clone(),
            recovery_id: None,
        })
        .unwrap()
        .record
        .unwrap();
    assert_eq!(found.context.request, request);
    assert_eq!(
        app.inspect_v4_project(&InspectV4Project {
            project_id: request.project_id.clone(),
            revision: None
        })
        .unwrap(),
        before
    );
    assert_eq!(
        app.connection
            .query_row("SELECT COUNT(*) FROM font_derivations_v1", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        app.ack_derived_font_recovery_v1(&AckDerivedFontRecoveryV1 {
            project_id: request.project_id,
            recovery_id: first.recovery_id
        })
        .unwrap_err()
        .code,
        "RECOVERY_UNRESOLVED"
    );
}
