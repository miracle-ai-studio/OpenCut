use super::*;
use opencut_contracts::edit_recovery::*;
use opencut_contracts::{CreateProject, FrameRate};
use opencut_editor_application::font_named as renamed;
fn fixture() -> (EditorApplication, ImportFont) {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let snapshot = app
        .create_scene_project(&CreateProjectRequest {
            project: CreateProject {
                id: "p".into(),
                name: "p".into(),
                sequence_id: "s".into(),
                width: 64,
                height: 48,
                fps: FrameRate {
                    numerator: 25,
                    denominator: 1,
                },
            },
            idempotency_key: "project".into(),
        })
        .unwrap();
    let request = ImportFont {
        project_id: "p".into(),
        expected_revision: snapshot.revision,
        idempotency_key: "original-renamed".into(),
        catalog_id: renamed::compiled_sources().unwrap()[0]
            .source
            .face
            .id
            .clone(),
        font_id: "font".into(),
        required_text: "AV".into(),
    };
    (app, request)
}
#[test]
fn same_immutable_proof_table_retains_v2_and_rejects_provider_version_spoofing() {
    let (app, request) = fixture();
    let source = renamed::source(&request.catalog_id).unwrap().source;
    let font = renamed::expected_import_resource(&request).unwrap();
    persist_verified_source(&app.connection, &font, &source).unwrap();
    persist_verified_source(&app.connection, &font, &source).unwrap();
    assert_eq!(
        read_source_for_font(&app.connection, &font).unwrap(),
        source
    );
    assert_eq!(
        app.connection
            .query_row("SELECT COUNT(*) FROM font_derivations", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        1
    );
    let mut forged = font.clone();
    if let ResourceOrigin::Generated { provider, .. } = &mut forged.origin {
        *provider = derived::OUTLINE_FONT_PROVIDER.into();
    }
    assert!(read_source_for_font(&app.connection, &forged).is_err());
    assert!(
        app.connection
            .execute("UPDATE font_derivations SET source_json='{}'", [])
            .is_err()
    );
}
#[test]
fn original_v2_request_stages_and_recovers_through_existing_exclusive_slot_without_font_io() {
    let (mut app, request) = fixture();
    let input = StageEditRecovery {
        context: EditRecoveryContext {
            intent: EditRecoveryIntent::FontImport {
                request: request.clone(),
            },
            sequence_id: Some("s".into()),
        },
    };
    let before = app
        .inspect_scene_project(&opencut_contracts::project_inspection::InspectProject {
            project_id: "p".into(),
            revision: None,
        })
        .unwrap();
    let staged = app.stage_edit_recovery(&input).unwrap();
    let again = app.stage_edit_recovery(&input).unwrap();
    assert_eq!(staged, again);
    let found = app
        .inspect_edit_recovery(&ReadEditRecovery {
            project_id: "p".into(),
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
        app.inspect_scene_project(&opencut_contracts::project_inspection::InspectProject {
            project_id: "p".into(),
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
}
