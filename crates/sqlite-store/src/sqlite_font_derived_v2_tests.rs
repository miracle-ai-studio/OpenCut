use super::*;
use opencut_contracts::{CreateProject, FrameRate};
use opencut_editor_application::font_derived_v2 as renamed;
fn fixture() -> (EditorApplication, ImportDerivedFontV1) {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let snapshot = app
        .create_v4_project(&CreateV4Project {
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
    let request = ImportDerivedFontV1 {
        project_id: "p".into(),
        expected_revision: snapshot.revision,
        idempotency_key: "original-renamed".into(),
        catalog_id: renamed::compiled_sources_v2().unwrap()[0]
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
    let font = renamed::expected_import_resource_v2(&request).unwrap();
    persist_verified_source(&app.connection, &font, &source).unwrap();
    persist_verified_source(&app.connection, &font, &source).unwrap();
    assert_eq!(
        read_source_for_font(&app.connection, &font).unwrap(),
        source
    );
    assert_eq!(
        app.connection
            .query_row("SELECT COUNT(*) FROM font_derivations_v1", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        1
    );
    let mut forged = font.clone();
    if let ResourceOriginV4::Generated { provider, .. } = &mut forged.origin {
        *provider = derived::DERIVED_FONT_PROVIDER_V1.into();
    }
    assert!(read_source_for_font(&app.connection, &forged).is_err());
    assert!(
        app.connection
            .execute("UPDATE font_derivations_v1 SET source_json='{}'", [])
            .is_err()
    );
}
#[test]
fn original_v2_request_stages_and_recovers_through_existing_exclusive_slot_without_font_io() {
    let (mut app, request) = fixture();
    let input = StageDerivedFontRecoveryV1 {
        context: DerivedFontRecoveryContextV1 {
            request: request.clone(),
            sequence_id: Some("s".into()),
        },
    };
    let before = app
        .inspect_v4_project(&opencut_contracts::project_v4_ops::InspectV4Project {
            project_id: "p".into(),
            revision: None,
        })
        .unwrap();
    let staged = app.stage_derived_font_recovery_v1(&input).unwrap();
    let again = app.stage_derived_font_recovery_v1(&input).unwrap();
    assert_eq!(staged, again);
    let found = app
        .inspect_derived_font_recovery_v1(&ReadDerivedFontRecoveryV1 {
            project_id: "p".into(),
            recovery_id: None,
        })
        .unwrap()
        .record
        .unwrap();
    assert_eq!(found.context.request, request);
    assert_eq!(
        app.inspect_v4_project(&opencut_contracts::project_v4_ops::InspectV4Project {
            project_id: "p".into(),
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
}
