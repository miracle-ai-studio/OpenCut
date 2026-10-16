use super::*;
use opencut_contracts::{CreateProject, FrameRate, text_frame_v2::*, text_frame_v3::*};
use serde_json::json;
use sha2::{Digest, Sha256};
fn hash(s: &str) -> String {
    format!("{:x}", Sha256::digest(s.as_bytes()))
}
fn ass(body: &str) -> String {
    format!(
        "[Script Info]\nPlayResX: 320\nPlayResY: 180\n[V4+ Styles]\nFormat: Name,Fontname,Fontsize,PrimaryColour,BackColour,Bold,Italic,Underline,StrikeOut,ScaleX,ScaleY,Spacing,Angle,BorderStyle,Outline,Shadow,Alignment,MarginL,MarginR,MarginV\nStyle: Default,Lato,24,&H00FFFFFF,&H80000000,0,0,0,0,100,100,0,0,1,0,0,2,10,20,30\n[Events]\nFormat: Layer,Start,End,Style,Name,MarginL,MarginR,MarginV,Effect,Text\nDialogue: 0,0:00:01.25,0:00:03.50,Default,,0,0,0,,{body}\n"
    )
}
fn fixture() -> (ProjectSnapshotV4, SubtitleClassicAssPlanRequestV1) {
    let mut project = crate::project_v4::new_document(&CreateProject {
        id: "p".into(),
        name: "Classic ASS".into(),
        sequence_id: "s".into(),
        width: 320,
        height: 180,
        fps: FrameRate {
            numerator: 24,
            denominator: 1,
        },
    })
    .unwrap();
    project.fonts.push(serde_json::from_value(json!({"id":"f","family":"Lato","weight":400,"style":"normal","face_index":0,"content":{"id":format!("av_{}","a".repeat(64)),"content_hash":"a".repeat(64),"size":100},"origin":{"kind":"legacy_locator","uri":"/private/fixed.ttf"},"license":null})).unwrap());
    (
        ProjectSnapshotV4 {
            project,
            revision: "r".into(),
        },
        SubtitleClassicAssPlanRequestV1 {
            project_id: "p".into(),
            expected_revision: "r".into(),
            sequence_id: "s".into(),
            idempotency_key: "classic-ass".into(),
            track: TextTrackTargetV1::New {
                track_id: "subtitles".into(),
            },
            content: ass("A B"),
            font_bindings: vec![],
            observations: None,
        },
    )
}
fn prepare(
    s: &ProjectSnapshotV4,
    r: &SubtitleClassicAssPlanRequestV1,
) -> Result<PreparedSubtitleClassicAssV1, EditorError> {
    prepare_with_checkpoint(s, r, &mut || Ok(()))
}
fn layers(p: &PreparedSubtitleClassicAssV1) -> Vec<TextLayerPlanV2> {
    p.frames()
        .iter()
        .map(|frame| {
            let t = &frame.texts()[0];
            TextLayerPlanV2 {
                element_id: t.identity.element_id.clone(),
                text: crate::classic_text_frame_v2::text_input(&t.text),
                font: ValidatedTextFontV2 {
                    font_id: t.font.id.clone(),
                    content_hash: t.font.content.content_hash.clone(),
                    size: t.font.content.size.to_string(),
                    weight: t.font.weight,
                    style: t.font.style,
                    face_index: t.font.face_index,
                    glyph_coverage_validated: true,
                    validated_text_hash: hash(&t.text.content),
                },
                opacity: t.opacity,
            }
        })
        .collect()
}
#[test]
fn classic_parser_retains_real_mapped_styles_border_three_and_warned_losses() {
    let source = ass("{\\move(0,0,10,10)} A\\N B{\\b1}\\hC")
        .replace(
            ",0,0,0,0,100,100,0,0,1,0,0,2,10,20,30",
            ",-1,-1,-1,-1,200,50,1.75,30,3,2,4,7,10,20,30",
        )
        .replace("Default,,0,0,0,,", "DEFAULT,,0,0,0,scroll,")
        + "Comment: ignored\n";
    let p = parse::parse(&source).unwrap();
    assert_eq!(p.cues.len(), 1);
    let c = &p.cues[0];
    assert_eq!(c.text, "A\n B C");
    assert_eq!(c.style.weight, FontWeightV4::Bold);
    assert_eq!(c.style.style, FontStyleV4::Italic);
    assert_eq!(c.style.decoration, TextDecorationV4::Underline);
    assert_eq!(c.style.spacing, 1.75);
    assert!((c.style.size - 11.97).abs() < 1e-10);
    assert!(c.style.background.enabled);
    assert_eq!(c.style.background.color.alpha, 0.498);
    assert_eq!(c.style.align, TextAlignV4::Left);
    assert_eq!(c.style.layout.vertical_align, 2);
    assert_eq!(c.style.layout.vertical_margin_ratio, 1. / 6.);
    assert_eq!(p.warnings.len(), 4);
    assert!(p.warnings[0].contains("Stripped unsupported ASS inline"));
    assert!(p.warnings[1].contains("event effects"));
    assert!(p.warnings[2].contains("non-dialogue"));
    assert!(p.warnings[3].contains("outline, shadow, rotation, or scaling"));
}
#[test]
fn defaults_legacy_styles_fractional_clocks_and_missing_style_fallback_match_classic() {
    let source = "[V4 Styles]\nFormat: Name,Fontname,Fontsize,Bold\nStyle: Default,Lato,24,0\n[Events]\nFormat: Start,End,Style,Text\nDialogue: 0:00:01.1,0:00:02.125,Missing,Hello\nDialogue: 0:00:03.00,0:00:04.00,default,World\nDialogue: bad,0:00:05.00,default,skip\nDialogue: 0:00:06.00,0:00:07.00,default,{\\b1}\n";
    let p = parse::parse(source).unwrap();
    assert_eq!(p.cues.len(), 2);
    assert_eq!(p.skipped, 2);
    assert_eq!(
        p.cues[0].range.start,
        Time {
            ticks: "132000".into(),
            timebase: 120000
        }
    );
    assert_eq!(
        p.cues[0].range.duration,
        Time {
            ticks: "123000".into(),
            timebase: 120000
        }
    );
    assert_eq!(p.cues[0].style.family, "Lato");
    assert!((p.cues[0].style.size - 7.47).abs() < 1e-10);
    assert!(p.warnings.iter().any(|w| w.contains("missing ASS styles")));
    let p = parse::parse("[Events]\nFormat: Start,End,Text\nDialogue: 0:00:00.0,0:00:01.0,Default")
        .unwrap();
    assert_eq!(p.cues[0].style.family, "Arial");
    assert_eq!(p.cues[0].style.weight, FontWeightV4::Bold);
    assert_eq!(p.cues[0].style.size, 5.);
}
#[test]
fn all_wrapstyle_values_and_absent_wrap_use_the_same_classic_greedy_directive() {
    let (s, r) = fixture();
    let before = s.clone();
    for wrap in [None, Some(0), Some(1), Some(2), Some(3)] {
        let mut r = r.clone();
        if let Some(w) = wrap {
            r.content = r
                .content
                .replace("[Script Info]", &format!("[Script Info]\nWrapStyle: {w}"));
        }
        let p = prepare(&s, &r).unwrap();
        let result = finish(&p, &r, &layers(&p)).unwrap();
        assert!(result.transaction.is_none());
        assert_eq!(result.cues.len(), 1);
        assert_eq!(result.cues[0].directive.max_width_px, 290.);
        assert_eq!(result.cues[0].placement.left_margin_px, 10.);
        assert_eq!(result.cues[0].placement.right_margin_px, 20.);
        assert_eq!(result.cues[0].placement.vertical_margin_px, 30.);
    }
    assert_eq!(s, before);
}
#[test]
fn unresolved_and_explicit_font_aliases_never_acquire_system_font_bytes() {
    let (mut s, r) = fixture();
    s.project.fonts.clear();
    let p = prepare(&s, &r).unwrap();
    assert!(p.frames().is_empty());
    let result = finish(&p, &r, &[]).unwrap();
    assert!(result.transaction.is_none());
    assert_eq!(result.font_requirements[0].family, "Lato");
    assert_eq!(result.font_requirements[0].resolved_font_id, None);
    let (s, mut r) = fixture();
    r.content = r.content.replace("Default,Lato,", "Default,Missing Face,");
    r.font_bindings.push(
        opencut_contracts::subtitle_ass_v1::SubtitleAssFontBindingV1 {
            family: "Missing Face".into(),
            weight: FontWeightV4::Normal,
            style: FontStyleV4::Normal,
            font_id: "f".into(),
        },
    );
    let p = prepare(&s, &r).unwrap();
    assert_eq!(p.frames().len(), 1);
    assert!(p.warnings.iter().any(|w| w.contains("Explicit font alias")));
    r.font_bindings[0].weight = FontWeightV4::Bold;
    assert!(prepare(&s, &r).is_err());
}
#[test]
fn shared_observation_kernel_owns_wrap_placement_and_one_atomic_transaction() {
    let (s, mut r) = fixture();
    let p = prepare(&s, &r).unwrap();
    let ls = layers(&p);
    let layer = &ls[0];
    let h = layer.text.line_height_px;
    let mut transform = layer.text.transform.clone();
    transform.center_x = 155.;
    transform.center_y = 150. - h / 2.;
    r.observations = Some(vec![SubtitleTextObservationV1 {
        caption: CaptionLayoutReceiptV3 {
            element_id: layer.element_id.clone(),
            original_text_hash: hash("A B"),
            wrapped_text: "A B".into(),
            measurements: vec![
                CaptionWrapMeasurementV3 {
                    content: "A B".into(),
                    width: 60.,
                },
                CaptionWrapMeasurementV3 {
                    content: "A B".into(),
                    width: 60.,
                },
            ],
        },
        raster: TextRasterReceiptV2 {
            element_id: layer.element_id.clone(),
            width: 320,
            height: 180,
            layout: TextLayoutV2 {
                font_id: "f".into(),
                font_content_hash: "a".repeat(64),
                validated_text_hash: hash("A B"),
                font_weight: 400,
                font_style: FontStyleV4::Normal,
                face_index: 0,
                font_size_px: layer.text.font_size_px,
                line_height_px: h,
                letter_spacing_px: 0.,
                align: "center".into(),
                baseline: "middle".into(),
                block: TextBlockMetricsV2 {
                    max_width: 60.,
                    height: h,
                    visual_center_offset: 0.,
                },
                background: None,
                lines: vec![TextLineMetricsV2 {
                    content: "A B".into(),
                    baseline_y: 0.,
                    width: 60.,
                    ascent: 20.,
                    descent: 5.,
                    decoration: None,
                }],
                transform,
            },
        },
    }]);
    let result = finish(&p, &r, &ls).unwrap();
    let tx = result.transaction.unwrap();
    assert_eq!(tx.operations.len(), 1);
    assert_eq!(tx.idempotency_key, r.idempotency_key);
    let opencut_contracts::project_v4_public::PublicProjectOperationV4::SequencePut { sequence } =
        &tx.operations[0]
    else {
        panic!()
    };
    let e = &sequence
        .tracks
        .iter()
        .find(|t| t.id == "subtitles")
        .unwrap()
        .elements[0];
    assert_eq!(e.visual.as_ref().unwrap().transform.position_x_px, -5.);
    assert_eq!(
        e.visual.as_ref().unwrap().transform.position_y_px,
        60. - h / 2.
    );
    r.observations.as_mut().unwrap()[0].caption.wrapped_text = "forged".into();
    assert!(finish(&p, &r, &ls).is_err());
}
#[test]
fn fixed_request_locks_cancellation_and_scope_fail_before_a_transaction() {
    let (s, r) = fixture();
    let p = prepare(&s, &r).unwrap();
    let mut bad = r.clone();
    bad.content.push_str("\n;change");
    assert!(finish(&p, &bad, &layers(&p)).is_err());
    let mut bad = r.clone();
    bad.expected_revision = "changed".into();
    assert!(prepare(&s, &bad).is_err());
    let mut locked = s.clone();
    locked.project.sequences[0].tracks[0].locked = true;
    let mut bad = r.clone();
    bad.track = TextTrackTargetV1::Existing {
        track_id: locked.project.sequences[0].tracks[0].id.clone(),
    };
    assert_eq!(prepare(&locked, &bad).unwrap_err().code, "TRACK_LOCKED");
    let error =
        prepare_with_checkpoint(&s, &r, &mut || Err(EditorError::new("CANCELLED", "cancel")))
            .unwrap_err();
    assert_eq!(error.code, "CANCELLED");
}
#[test]
fn exhausted_margins_preserve_zero_width_without_weakening_the_frozen_wrap_profile() {
    let (s, mut r) = fixture();
    r.content = r.content.replace(",2,10,20,30", ",2,200,200,30");
    let p = prepare(&s, &r).unwrap();
    let result = finish(&p, &r, &layers(&p)).unwrap();
    assert_eq!(result.cues[0].directive.max_width_px, 0.);
    let ms = vec![
        CaptionWrapMeasurementV3 {
            content: "A B".into(),
            width: 10.,
        },
        CaptionWrapMeasurementV3 {
            content: "A".into(),
            width: 5.,
        },
        CaptionWrapMeasurementV3 {
            content: "B".into(),
            width: 5.,
        },
    ];
    assert_eq!(
        crate::classic_text_frame_v3::verify_classic_wrap("A B", 0., &ms).unwrap(),
        "A\nB"
    );
    assert!(crate::classic_text_frame_v3::verify_wrap("A B", 0., &ms).is_err());
}

#[test]
fn distinct_cue_styles_keep_their_own_fixed_fonts_backgrounds_and_margins() {
    let (mut s, mut r) = fixture();
    let mut bold = s.project.fonts[0].clone();
    bold.id = "bold".into();
    bold.weight = 700;
    s.project.fonts.push(bold);
    r.content = r.content.replace(
        "[Events]",
        "Style: Box,Lato,30,&H00112233,&H80000000,-1,0,0,0,100,100,2.5,0,3,0,0,9,5,15,20\n[Events]",
    );
    r.content
        .push_str("Dialogue: 0,0:00:04.00,0:00:05.00,Box,,0,0,0,,Second style\n");
    let p = prepare(&s, &r).unwrap();
    assert_eq!(p.frames().len(), 2);
    let first = &p.frames()[0].texts()[0];
    let second = &p.frames()[1].texts()[0];
    assert_eq!(first.font.id, "f");
    assert_eq!(second.font.id, "bold");
    assert!(!first.text.background.enabled);
    assert!(second.text.background.enabled);
    assert_eq!(second.text.letter_spacing_px, 2.5);
    let result = finish(&p, &r, &layers(&p)).unwrap();
    assert_eq!(result.font_requirements.len(), 2);
    assert_eq!(
        result.cues[1].placement.vertical_align,
        SubtitleClassicVerticalAlignV1::Top
    );
    assert_eq!(result.cues[1].placement.left_margin_px, 5.);
    assert_eq!(result.cues[1].placement.right_margin_px, 15.);
}
