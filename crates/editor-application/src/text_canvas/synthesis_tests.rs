use super::*;
use crate::text_canvas::tests::fixture;
use opencut_contracts::font_derived::ImportFont;

fn synthesis_font(id: &str) -> FontResource {
    crate::font_synthesis::expected_import_resource(&ImportFont {
        project_id: "p".into(),
        expected_revision: "r".into(),
        idempotency_key: "canvas-font".into(),
        catalog_id: "google-gudea-bold-synthetic-italic-v1".into(),
        font_id: id.into(),
        required_text: "AV".into(),
    })
    .unwrap()
}
fn synthetic_fixture() -> (EditorSnapshot, TextCanvasPlanRequest) {
    let (mut snapshot, request) = fixture();
    snapshot.project.fonts[0] = synthesis_font("f");
    let ElementContent::Text { text } = &mut snapshot.project.sequences[0]
        .tracks
        .last_mut()
        .unwrap()
        .elements[0]
        .content
    else {
        unreachable!()
    };
    text.font_weight = FontWeight::Bold;
    text.font_style = FontStyle::Italic;
    text.content = "AV".into();
    (snapshot, request)
}
fn policy() -> LocalVisualExecutionPolicy {
    crate::media_video_jobs::tests::input()
        .processing_policy
        .visual
}
fn coverage(execution: &PreparedTextFrame) -> Vec<NativeTextCoverage> {
    execution
        .texts()
        .iter()
        .map(|text| NativeTextCoverage {
            element_id: text.identity.element_id.clone(),
            font: text.font.clone(),
            content: text.text.content.clone(),
        })
        .collect()
}

#[test]
fn canvas_keeps_actual_normal_font_and_explicit_italic_intent_without_mutating_source() {
    let (snapshot, request) = synthetic_fixture();
    let before = snapshot.clone();
    let prepared = prepare_canvas_plan(&snapshot, &request).unwrap();
    let original = binding_for_resource(&snapshot.project.fonts[0])
        .unwrap()
        .unwrap();
    assert_eq!(prepared.font_synthesis(), Some(&original));
    let execution = prepared.execution();
    assert_eq!(execution.texts()[0].font.style, FontStyle::Normal);
    assert_eq!(
        execution.texts()[0].font.content,
        snapshot.project.fonts[0].content
    );
    assert_eq!(execution.texts()[0].text.transform.scale_x, 2.0);
    assert_eq!(execution.texts()[0].text.transform.scale_y, 0.5);
    let result = canvas_plan_result(&prepared, &request, &policy(), &coverage(execution)).unwrap();
    assert_eq!(result.identity.profile, CLASSIC_TEXT_CANVAS_PROFILE);
    assert_eq!(result.layer.font.style, FontStyle::Normal);
    assert_eq!(result.layer.font.weight, 700);
    assert_eq!(
        result.font_synthesis.unwrap().requested.style,
        FontStyle::Italic
    );
    assert_eq!(snapshot, before);
}

#[test]
fn ordinary_canvas_keeps_native_metrics_without_a_synthesis_claim() {
    let (snapshot, request) = fixture();
    let prepared = prepare_canvas_plan(&snapshot, &request).unwrap();
    let result = canvas_plan_result(
        &prepared,
        &request,
        &policy(),
        &coverage(prepared.execution()),
    )
    .unwrap();
    assert_eq!(
        result.layer.font.content_hash,
        snapshot.project.fonts[0].content.content_hash
    );
    assert_eq!(result.font_synthesis, None);
    assert_eq!(result.identity.profile, CLASSIC_TEXT_CANVAS_PROFILE);
}
#[test]
fn canvas_checks_even_unused_bindings_and_does_not_rewrite_an_ordinary_target() {
    let (mut snapshot, request) = fixture();
    snapshot.project.fonts.push(synthesis_font("unused"));
    let before = snapshot.clone();
    let prepared = prepare_canvas_plan(&snapshot, &request).unwrap();
    assert_eq!(prepared.font_synthesis(), None);
    assert_eq!(
        prepared.execution().texts()[0].font,
        snapshot.project.fonts[0]
    );
    assert_eq!(snapshot, before);
    snapshot.project.fonts[1].license.as_mut().unwrap().notice = Some("forged".into());
    assert_eq!(
        prepare_canvas_plan(&snapshot, &request).unwrap_err().code,
        "FONT_SOURCE_INTEGRITY_ERROR"
    );
}

#[test]
fn canvas_preserves_fixed_scope_lock_activity_and_authored_style_rejection() {
    for mutation in 0..5 {
        let (mut snapshot, mut request) = synthetic_fixture();
        match mutation {
            0 => request.revision = "other".into(),
            1 => {
                snapshot.project.sequences[0]
                    .tracks
                    .last_mut()
                    .unwrap()
                    .locked = true
            }
            2 => request.time.ticks = "99".into(),
            3 => {
                if let ElementContent::Text { text } = &mut snapshot.project.sequences[0]
                    .tracks
                    .last_mut()
                    .unwrap()
                    .elements[0]
                    .content
                {
                    text.font_style = FontStyle::Normal;
                }
            }
            4 => snapshot.project.fonts[0].style = FontStyle::Italic,
            _ => unreachable!(),
        }
        let before = snapshot.clone();
        assert!(prepare_canvas_plan(&snapshot, &request).is_err());
        assert_eq!(snapshot, before);
    }
}

#[test]
fn canvas_result_requires_current_exact_coverage_scope_and_full_budget() {
    let (snapshot, request) = synthetic_fixture();
    let prepared = prepare_canvas_plan(&snapshot, &request).unwrap();
    assert!(canvas_plan_result(&prepared, &request, &policy(), &[]).is_err());
    let mut facts = coverage(prepared.execution());
    facts[0].content.push('!');
    assert!(canvas_plan_result(&prepared, &request, &policy(), &facts).is_err());
    let facts = coverage(prepared.execution());
    let mut other = request.clone();
    other.time.ticks = "1".into();
    assert!(canvas_plan_result(&prepared, &other, &policy(), &facts).is_err());
    let mut small = policy();
    small.max_encoded_bytes = 1;
    assert!(canvas_plan_result(&prepared, &request, &small, &facts).is_err());
}
