//! White-box persistence/recovery fixtures. No font bytes, native parsing or
//! browser synthesis execution is claimed by these tests.
use super::*;
use opencut_contracts::edit_recovery::*;
use opencut_contracts::{CreateProject, FrameRate};
use opencut_editor_application::font_synthesis as synthesis;
fn fixture() -> (EditorApplication, EditorSnapshot, ImportFont) {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let snapshot = app
        .create_scene_project(&CreateProjectRequest {
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
    let request = ImportFont {
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
    require_derived_proposal(
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
    tx.rollback().unwrap();
    assert_eq!(
        app.connection
            .query_row("SELECT COUNT(*) FROM font_derivations", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        0
    );
    let tx = app
        .connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    persist_verified_source(&tx, &font, &source).unwrap();
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
    let proof = app.read_derived_font_proof(&query).unwrap();
    assert_eq!(proof.source, source);
    let typed = synthesis::proof::read(&app, &query).unwrap();
    assert_eq!(typed.source.source, source);
    assert_eq!(typed.source.binding.requested.style, FontStyle::Italic);
    assert_eq!(typed.source.binding.actual.style, FontStyle::Normal);
    assert_eq!(derived::proof::read(&app, &query).unwrap(), proof);
    assert_eq!(
        opencut_editor_application::font_derived::proof::read(&app, &query).unwrap(),
        proof
    );
    assert_eq!(
        app.connection
            .query_row("SELECT COUNT(*) FROM font_derivations", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        1
    );
    assert!(
        app.connection
            .execute("UPDATE font_derivations SET proof_json='{}'", [])
            .is_err()
    );
    assert!(
        app.connection
            .execute("DELETE FROM font_derivations", [])
            .is_err()
    );
    let mut forged = font;
    let ResourceOrigin::Generated { provider, .. } = &mut forged.origin else {
        panic!()
    };
    *provider = derived::OUTLINE_FONT_PROVIDER.into();
    assert!(read_source_for_font(&app.connection, &forged).is_err());
}
#[test]
fn synthesis_recovery_retains_original_request_without_writing_proof_or_revision() {
    let (mut app, before, request) = fixture();
    let stage = StageEditRecovery {
        context: EditRecoveryContext {
            intent: EditRecoveryIntent::FontImport {
                request: request.clone(),
            },
            sequence_id: Some("main".into()),
        },
    };
    let first = app.stage_edit_recovery(&stage).unwrap();
    assert_eq!(app.stage_edit_recovery(&stage).unwrap(), first);
    let found = app
        .inspect_edit_recovery(&ReadEditRecovery {
            project_id: request.project_id.clone(),
            recovery_id: None,
        })
        .unwrap()
        .record
        .unwrap();
    assert_eq!(
        found.context.intent,
        EditRecoveryIntent::FontImport {
            request: request.clone()
        }
    );
    assert_eq!(
        app.inspect_scene_project(&InspectProject {
            project_id: request.project_id.clone(),
            revision: None
        })
        .unwrap(),
        before
    );
    assert_eq!(
        app.connection
            .query_row("SELECT COUNT(*) FROM font_derivations", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        app.ack_edit_recovery(&AckEditRecovery {
            project_id: request.project_id,
            recovery_id: first.recovery_id
        })
        .unwrap_err()
        .code,
        "RECOVERY_UNRESOLVED"
    );
}
