use super::*;
use opencut_contracts::{CreateProject, FrameRate, project_v4_ops::CreateV4Project};
#[test]
fn v2_source_is_registered_and_old_public_entrypoints_remain_v1_only() {
    let tmp = tempfile::tempdir().unwrap();
    let mut host = Automation::open(tmp.path().join("workspace.sqlite")).unwrap();
    let snapshot = host
        .app
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
    let pin = application::compiled_sources_v2().unwrap().first().unwrap();
    let public = host.call(
        "opencut_font_derived_v2_source",
        serde_json::json!({"catalog_id":pin.source.face.id}),
    );
    assert!(public.error.is_none());
    assert_eq!(
        public.data.unwrap(),
        serde_json::to_value(&pin.source).unwrap()
    );
    assert_eq!(
        host.call(
            "opencut_font_derived_v1_source",
            serde_json::json!({"catalog_id":pin.source.face.id})
        )
        .error
        .unwrap()
        .code,
        "NOT_FOUND"
    );
    let request = ImportDerivedFontV1 {
        project_id: "p".into(),
        expected_revision: snapshot.revision.clone(),
        idempotency_key: "original-font".into(),
        catalog_id: pin.source.face.id.clone(),
        font_id: "font".into(),
        required_text: "A".into(),
    };
    let before = host
        .call(
            "opencut_project_v4_history_navigation",
            serde_json::json!({"project_id":"p","revision":null}),
        )
        .data;
    assert_eq!(
        host.call(
            "opencut_font_derived_v1_import",
            serde_json::to_value(&request).unwrap()
        )
        .error
        .unwrap()
        .code,
        "NOT_FOUND"
    );
    let mut stale = request.clone();
    stale.expected_revision = "wrong-revision".into();
    assert_eq!(
        host.call(
            "opencut_font_derived_v2_import",
            serde_json::to_value(stale).unwrap()
        )
        .error
        .unwrap()
        .code,
        "REVISION_CONFLICT"
    );
    assert_eq!(
        before,
        host.call(
            "opencut_project_v4_history_navigation",
            serde_json::json!({"project_id":"p","revision":null})
        )
        .data
    );
}
