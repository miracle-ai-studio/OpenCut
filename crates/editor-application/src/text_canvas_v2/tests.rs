use super::*;
use crate::text_canvas_v1::{prepare_canvas_plan_v1, tests::fixture};
use opencut_contracts::font_derived_v1::ImportDerivedFontV1;

fn synthesis_font(id: &str) -> FontResourceV4 {
    crate::font_synthesis_v1::expected_import_resource(&ImportDerivedFontV1 {
        project_id: "p".into(),
        expected_revision: "r".into(),
        idempotency_key: "canvas-font".into(),
        catalog_id: "google-gudea-bold-synthetic-italic-v1".into(),
        font_id: id.into(),
        required_text: "AV".into(),
    })
    .unwrap()
}
fn synthetic_fixture() -> (ProjectSnapshotV4, TextCanvasPlanRequestV1) {
    let (mut snapshot, request) = fixture();
    snapshot.project.fonts[0] = synthesis_font("f");
    let ElementContentV4::Text { text } = &mut snapshot.project.sequences[0]
        .tracks
        .last_mut()
        .unwrap()
        .elements[0]
        .content
    else {
        unreachable!()
    };
    text.font_weight = FontWeightV4::Bold;
    text.font_style = FontStyleV4::Italic;
    text.content = "AV".into();
    (snapshot, request)
}
fn policy() -> LocalVisualExecutionPolicyV1 {
    crate::project_v4_video_jobs::tests::input()
        .processing_policy
        .visual
}
fn coverage(execution: &PreparedTextFrameV2) -> Vec<NativeTextCoverageV1> {
    execution
        .texts()
        .iter()
        .map(|text| NativeTextCoverageV1 {
            element_id: text.identity.element_id.clone(),
            font: text.font.clone(),
            content: text.text.content.clone(),
        })
        .collect()
}

#[test]
fn canvas_v2_keeps_actual_normal_font_and_explicit_italic_intent_without_mutating_source() {
    let (snapshot, request) = synthetic_fixture();
    let before = snapshot.clone();
    let prepared = prepare_canvas_plan_v2(&snapshot, &request).unwrap();
    let original = binding_for_resource(&snapshot.project.fonts[0])
        .unwrap()
        .unwrap();
    assert_eq!(prepared.font_synthesis(), Some(&original));
    let execution = prepared.execution();
    assert_eq!(execution.texts()[0].font.style, FontStyleV4::Normal);
    assert_eq!(
        execution.texts()[0].font.content,
        snapshot.project.fonts[0].content
    );
    assert_eq!(execution.texts()[0].text.transform.scale_x, 2.0);
    assert_eq!(execution.texts()[0].text.transform.scale_y, 0.5);
    let result =
        canvas_plan_result_v2(&prepared, &request, &policy(), &coverage(execution)).unwrap();
    assert_eq!(result.schema_version, 2);
    assert_eq!(result.identity.profile, CLASSIC_TEXT_CANVAS_PROFILE_V2);
    assert_eq!(result.layer.font.style, FontStyleV4::Normal);
    assert_eq!(result.layer.font.weight, 700);
    assert_eq!(
        result.font_synthesis.unwrap().requested.style,
        FontStyleV4::Italic
    );
    assert_eq!(snapshot, before);
}

#[test]
fn normal_canvas_v2_reuses_the_v1_plan_and_coverage_with_no_synthesis_claim() {
    let (snapshot, request) = fixture();
    let old = prepare_canvas_plan_v1(&snapshot, &request).unwrap();
    let new = prepare_canvas_plan_v2(&snapshot, &request).unwrap();
    assert_eq!(old.graph(), new.execution().graph());
    let coverage = coverage(&old);
    let old = canvas_plan_result_v1(&old, &request, &policy(), &coverage).unwrap();
    let new = canvas_plan_result_v2(&new, &request, &policy(), &coverage).unwrap();
    assert_eq!(old.layer, new.layer);
    assert_eq!(new.font_synthesis, None);
    assert_eq!(new.identity.profile, CLASSIC_TEXT_CANVAS_PROFILE_V2);
}

#[test]
fn canvas_v1_rejects_synthesis_for_blank_inactive_and_unreferenced_resources() {
    for state in 0..4 {
        let (mut snapshot, request) = synthetic_fixture();
        let track = snapshot.project.sequences[0].tracks.last_mut().unwrap();
        match state {
            0 => {
                if let ElementContentV4::Text { text } = &mut track.elements[0].content {
                    text.content = " \n".into();
                }
            }
            1 => track.enabled = false,
            2 => track.elements[0].timeline_range.start.ticks = "99".into(),
            3 => {
                let (mut ordinary, _) = fixture();
                ordinary.project.fonts.push(synthesis_font("unused"));
                snapshot = ordinary;
            }
            _ => unreachable!(),
        }
        let error = prepare_canvas_plan_v1(&snapshot, &request).unwrap_err();
        assert_eq!(error.code, "UNSUPPORTED_V4");
        assert!(error.message.contains("Explicit font synthesis"));
    }
}

#[test]
fn canvas_v2_checks_even_unused_bindings_and_does_not_rewrite_an_ordinary_target() {
    let (mut snapshot, request) = fixture();
    snapshot.project.fonts.push(synthesis_font("unused"));
    let before = snapshot.clone();
    let prepared = prepare_canvas_plan_v2(&snapshot, &request).unwrap();
    assert_eq!(prepared.font_synthesis(), None);
    assert_eq!(
        prepared.execution().texts()[0].font,
        snapshot.project.fonts[0]
    );
    assert_eq!(snapshot, before);
    snapshot.project.fonts[1].license.as_mut().unwrap().notice = Some("forged".into());
    assert_eq!(
        prepare_canvas_plan_v2(&snapshot, &request)
            .unwrap_err()
            .code,
        "FONT_SOURCE_INTEGRITY_ERROR"
    );
}

#[test]
fn canvas_v2_preserves_fixed_scope_lock_activity_and_authored_style_rejection() {
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
                if let ElementContentV4::Text { text } = &mut snapshot.project.sequences[0]
                    .tracks
                    .last_mut()
                    .unwrap()
                    .elements[0]
                    .content
                {
                    text.font_style = FontStyleV4::Normal;
                }
            }
            4 => snapshot.project.fonts[0].style = FontStyleV4::Italic,
            _ => unreachable!(),
        }
        let before = snapshot.clone();
        assert!(prepare_canvas_plan_v2(&snapshot, &request).is_err());
        assert_eq!(snapshot, before);
    }
}

#[test]
fn canvas_v2_result_requires_current_exact_coverage_scope_and_full_budget() {
    let (snapshot, request) = synthetic_fixture();
    let prepared = prepare_canvas_plan_v2(&snapshot, &request).unwrap();
    assert!(canvas_plan_result_v2(&prepared, &request, &policy(), &[]).is_err());
    let mut facts = coverage(prepared.execution());
    facts[0].content.push('!');
    assert!(canvas_plan_result_v2(&prepared, &request, &policy(), &facts).is_err());
    let facts = coverage(prepared.execution());
    let mut other = request.clone();
    other.time.ticks = "1".into();
    assert!(canvas_plan_result_v2(&prepared, &other, &policy(), &facts).is_err());
    let mut small = policy();
    small.max_encoded_bytes = 1;
    assert!(canvas_plan_result_v2(&prepared, &request, &small, &facts).is_err());
}
