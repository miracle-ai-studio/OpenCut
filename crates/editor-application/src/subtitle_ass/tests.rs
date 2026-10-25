use super::*;
use opencut_contracts::{
    CreateProject, FrameRate, subtitle_text::SubtitleTextExport, subtitles::SubtitleFormat,
    text_frame::*,
};
use serde_json::json;
fn ass(body: &str) -> String {
    format!(
        "[Script Info]\nScriptType: v4.00+\nPlayResX: 320\nPlayResY: 180\nWrapStyle: 2\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Lato,24,&H00FFFFFF,&H00FFFFFF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,0,0,2,10,20,30,1\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.25,0:00:03.50,Default,,0,0,0,,{body}\n"
    )
}
fn fixture() -> (EditorSnapshot, SubtitleAssPlanRequest) {
    let mut project = crate::scene::new_document(&CreateProject {
        id: "p".into(),
        name: "ASS".into(),
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
    let request = SubtitleAssPlanRequest {
        project_id: "p".into(),
        expected_revision: "r".into(),
        sequence_id: "s".into(),
        idempotency_key: "ass-import".into(),
        track: TextTrackTarget::New {
            track_id: "subtitles".into(),
        },
        content: ass("{\\an5\\pos(100,80)} A  B\\N C "),
        font_bindings: vec![],
        observations: None,
    };
    (
        EditorSnapshot {
            project,
            revision: "r".into(),
        },
        request,
    )
}
fn prepare(
    snapshot: &EditorSnapshot,
    request: &SubtitleAssPlanRequest,
) -> Result<PreparedSubtitleAss, EditorError> {
    prepare_with_checkpoint(snapshot, request, &mut || Ok(()))
}
fn layers(prepared: &PreparedSubtitleAss) -> Vec<TextLayerPlan> {
    prepared
        .frames
        .iter()
        .map(|frame| {
            let original = &frame.texts()[0];
            TextLayerPlan {
                element_id: original.identity.element_id.clone(),
                text: crate::classic_text_frame::text_input(&original.text),
                font: ValidatedTextFont {
                    font_id: original.font.id.clone(),
                    content_hash: original.font.content.content_hash.clone(),
                    size: original.font.content.size.to_string(),
                    weight: original.font.weight,
                    style: original.font.style,
                    face_index: original.font.face_index,
                    glyph_coverage_validated: true,
                    validated_text_hash: hash(&original.text.content),
                },
                opacity: original.opacity,
            }
        })
        .collect()
}
fn observations(
    prepared: &PreparedSubtitleAss,
    layers: &[TextLayerPlan],
) -> Vec<TextRasterReceipt> {
    layers
        .iter()
        .map(|layer| {
            let lines = layer.text.content.split('\n').collect::<Vec<_>>();
            let center = (lines.len() - 1) as f64 * layer.text.line_height_px / 2.0;
            TextRasterReceipt {
                element_id: layer.element_id.clone(),
                width: prepared.width,
                height: prepared.height,
                layout: TextLayout {
                    font_id: layer.font.font_id.clone(),
                    font_content_hash: layer.font.content_hash.clone(),
                    validated_text_hash: layer.font.validated_text_hash.clone(),
                    font_weight: layer.font.weight,
                    font_style: layer.font.style,
                    face_index: layer.font.face_index,
                    font_size_px: layer.text.font_size_px,
                    line_height_px: layer.text.line_height_px,
                    letter_spacing_px: layer.text.letter_spacing_px,
                    align: layer.text.align.clone(),
                    baseline: "middle".into(),
                    block: TextBlockMetrics {
                        max_width: 60.0,
                        height: lines.len() as f64 * layer.text.line_height_px,
                        visual_center_offset: center,
                    },
                    background: None,
                    lines: lines
                        .iter()
                        .enumerate()
                        .map(|(i, s)| TextLineMetrics {
                            content: (*s).into(),
                            baseline_y: i as f64 * layer.text.line_height_px - center,
                            width: 60.0,
                            ascent: 15.0,
                            descent: 3.0,
                            decoration: None,
                        })
                        .collect(),
                    transform: layer.text.transform.clone(),
                },
            }
        })
        .collect()
}
#[test]
fn subtitle_ass_exact_font_and_q2_content_commit_once_with_original_graph() {
    let (snapshot, mut request) = fixture();
    let original = snapshot.clone();
    let prepared = prepare(&snapshot, &request).unwrap();
    let layers = layers(&prepared);
    let first = finish(&prepared, &request, &layers).unwrap();
    assert!(first.transaction.is_none());
    assert_eq!(
        first.font_requirements[0].resolved_font_id,
        Some("f".into())
    );
    assert_eq!(layers[0].text.content, " A  B\n C ");
    assert_eq!(layers[0].text.transform.center_x, 100.0);
    assert_eq!(layers[0].text.transform.center_y, 80.0);
    request.observations = Some(observations(&prepared, &layers));
    let result = finish(&prepared, &request, &layers).unwrap();
    let transaction = result.transaction.unwrap();
    assert_eq!(transaction.idempotency_key, request.idempotency_key);
    assert_eq!(transaction.operations.len(), 1);
    let ProjectEditOperation::SequencePut { sequence } = &transaction.operations[0] else {
        panic!()
    };
    assert_eq!(sequence.tracks[0], snapshot.project.sequences[0].tracks[0]);
    assert_eq!(sequence.captions, snapshot.project.sequences[0].captions);
    assert_eq!(sequence.bookmarks, snapshot.project.sequences[0].bookmarks);
    assert_eq!(
        sequence.transitions,
        snapshot.project.sequences[0].transitions
    );
    let element = &sequence
        .tracks
        .iter()
        .find(|t| t.id == "subtitles")
        .unwrap()
        .elements[0];
    assert_eq!(
        element.visual.as_ref().unwrap().transform.position_x_px,
        -60.0
    );
    assert_eq!(
        element.visual.as_ref().unwrap().transform.position_y_px,
        -10.0
    );
    assert_eq!(snapshot, original);
    assert_eq!(
        prepared.ids,
        prepare(
            &snapshot,
            &SubtitleAssPlanRequest {
                observations: None,
                ..request
            }
        )
        .unwrap()
        .ids
    );
}
#[test]
fn subtitle_ass_missing_fonts_aliases_and_face_identity_are_explicit() {
    let (mut snapshot, mut request) = fixture();
    request.content = ass("{\\fnMissing}A");
    let missing = prepare(&snapshot, &request).unwrap();
    assert!(missing.frames().is_empty());
    let result = finish(&missing, &request, &[]).unwrap();
    assert_eq!(result.font_requirements[0].family, "Missing");
    assert!(result.font_requirements[0].resolved_font_id.is_none());
    assert!(result.cues.is_empty() && result.transaction.is_none() && result.selection.is_none());
    request.font_bindings = vec![SubtitleAssFontBinding {
        family: "Missing".into(),
        weight: FontWeight::Normal,
        style: FontStyle::Normal,
        font_id: "f".into(),
    }];
    let alias = prepare(&snapshot, &request).unwrap();
    assert_eq!(alias.frames().len(), 1);
    assert!(
        alias
            .warnings
            .iter()
            .any(|w| w.contains("Explicit font alias"))
    );
    request.font_bindings.push(request.font_bindings[0].clone());
    assert!(prepare(&snapshot, &request).is_err());
    request.font_bindings.pop();
    request.font_bindings[0].family = "Unrelated".into();
    assert!(prepare(&snapshot, &request).is_err());
    request.font_bindings[0].family = "Missing".into();
    snapshot.project.fonts[0].weight = 700;
    assert!(prepare(&snapshot, &request).is_err());
}
#[test]
fn subtitle_ass_vertical_anchors_transform_local_offset_through_rotation_scale() {
    let (snapshot, mut request) = fixture();
    for an in 1..=9 {
        request.content = ass(&format!(
            "{{\\an{an}\\pos(100,80)\\fscx75\\fscy75\\frz-90}}A\\NB"
        ));
        let prepared = prepare(&snapshot, &request).unwrap();
        let layers = layers(&prepared);
        request.observations = Some(observations(&prepared, &layers));
        let result = finish(&prepared, &request, &layers).unwrap();
        let ProjectEditOperation::SequencePut { sequence } =
            &result.transaction.unwrap().operations[0]
        else {
            panic!()
        };
        let element = &sequence
            .tracks
            .iter()
            .find(|t| t.id == "subtitles")
            .unwrap()
            .elements[0];
        let tr = &element.visual.as_ref().unwrap().transform;
        let delta = match (an - 1) / 3 {
            0 => -18.0,
            1 => 0.0,
            _ => 18.0,
        };
        assert!((tr.position_x_px - (-60.0 - delta)).abs() < 1e-10);
        assert!((tr.position_y_px + 10.0).abs() < 1e-10);
        assert_eq!(tr.rotation_degrees, 90.0);
        request.observations = None;
    }
    request.content = ass("A\\NB");
    let prepared = prepare(&snapshot, &request).unwrap();
    let layers = layers(&prepared);
    assert_eq!(layers[0].text.transform.center_x, 160.0);
    assert_eq!(layers[0].text.transform.center_y, 150.0);
    request.observations = Some(observations(&prepared, &layers));
    let result = finish(&prepared, &request, &layers).unwrap();
    assert_eq!(result.cues[0].layer.text.transform.center_y, 126.0);
}
#[test]
fn subtitle_ass_playres_scale_preserves_script_geometry_without_skew() {
    let (snapshot, mut request) = fixture();
    request.content = ass("{\\an5\\fs24\\fsp2\\fscx125\\fscy75\\pos(50,40)}A")
        .replace("PlayResX: 320", "PlayResX: 160")
        .replace("PlayResY: 180", "PlayResY: 90");
    let prepared = prepare(&snapshot, &request).unwrap();
    let text = &prepared.frames()[0].texts()[0].text;
    assert_eq!(text.font_size_px, 48.0);
    assert_eq!(text.letter_spacing_px, 4.0);
    assert_eq!(text.transform.center_x, 100.0);
    assert_eq!(text.transform.center_y, 80.0);
    assert_eq!(text.transform.scale_x, 1.25);
    assert_eq!(text.transform.scale_y, 0.75);
    request.content = request.content.replace("PlayResX: 160", "PlayResX: 320");
    let prepared = prepare(&snapshot, &request).unwrap();
    assert_eq!(
        prepared.frames()[0].texts()[0].text.transform.scale_x,
        0.625
    );
    request.content = request.content.replace("\\an5", "\\an5\\frz30");
    assert_eq!(
        prepare(&snapshot, &request).unwrap_err().code,
        "SUBTITLE_ASS_UNSUPPORTED"
    );
}
#[test]
fn subtitle_ass_refuses_missing_swapped_forged_observations_and_unsupported_wrap() {
    let (snapshot, mut request) = fixture();
    let prepared = prepare(&snapshot, &request).unwrap();
    let layers = layers(&prepared);
    let good = observations(&prepared, &layers);
    request.observations = Some(vec![]);
    assert!(finish(&prepared, &request, &layers).is_err());
    for field in 0..4 {
        let mut bad = good.clone();
        match field {
            0 => bad[0].element_id = "other".into(),
            1 => bad[0].layout.transform.center_x += 1.0,
            2 => bad[0].layout.validated_text_hash = "b".repeat(64),
            _ => bad[0].width = 319,
        };
        request.observations = Some(bad);
        assert!(finish(&prepared, &request, &layers).is_err());
    }
    request.observations = None;
    for q in [0, 1, 3] {
        request.content = ass(&format!("{{\\q{q}}}A"));
        assert_eq!(
            prepare(&snapshot, &request).unwrap_err().code,
            "SUBTITLE_ASS_UNSUPPORTED"
        );
    }
    request.content = ass("{\\fscx125\\fscy75\\frz30}A");
    assert_eq!(
        prepare(&snapshot, &request).unwrap_err().code,
        "SUBTITLE_ASS_UNSUPPORTED"
    );
    request.content = ass("{\\fnMissing}A");
    request.track = TextTrackTarget::New {
        track_id: snapshot.project.sequences[0].main_track_id.clone(),
    };
    assert!(prepare(&snapshot, &request).is_err());
    request.track = TextTrackTarget::New {
        track_id: "subtitles".into(),
    };
    request.expected_revision = "stale".into();
    assert!(prepare(&snapshot, &request).is_err());
}
#[test]
fn subtitle_ass_actual_export_roundtrip_preserves_supported_style_position() {
    let (mut snapshot, mut request) = fixture();
    snapshot.project.fonts[0].weight = 700;
    snapshot.project.fonts[0].style = FontStyle::Italic;
    request.content = ass(
        "{\\fnLato\\b700\\i1\\fs27.5\\fsp1.5\\fscx75\\fscy75\\frz-30\\an5\\pos(192,78)\\1c&H1133CC&\\1a&H80&\\bord0\\shad0\\q2}Hello\\NWorld",
    );
    let prepared = prepare(&snapshot, &request).unwrap();
    let source = prepared.sequence.clone();
    let ElementContent::Text { text } = &source
        .tracks
        .iter()
        .find(|t| t.id == "subtitles")
        .unwrap()
        .elements[0]
        .content
    else {
        panic!()
    };
    assert_eq!(text.decoration, TextDecoration::None);
    snapshot.project.sequences[0] = source;
    let exported = crate::subtitle_text_export::export(
        &snapshot,
        &SubtitleTextExport {
            project_id: "p".into(),
            revision: "r".into(),
            sequence_id: "s".into(),
            track_id: "subtitles".into(),
            format: SubtitleFormat::Ass,
        },
    )
    .unwrap();
    request.track = TextTrackTarget::New {
        track_id: "roundtrip".into(),
    };
    request.content = exported.content;
    request.idempotency_key = "roundtrip".into();
    let roundtrip = prepare(&snapshot, &request).unwrap();
    let original = prepared.frames()[0].texts()[0].text.clone();
    let next = &roundtrip.frames()[0].texts()[0].text;
    assert_eq!(&original, next);
    assert_eq!(
        roundtrip.frames()[0].texts()[0].font,
        prepared.frames()[0].texts()[0].font
    );
}
#[test]
fn subtitle_ass_prepare_cancels_before_native_frames() {
    let (snapshot, request) = fixture();
    let mut checks = 0;
    let error = prepare_with_checkpoint(&snapshot, &request, &mut || {
        checks += 1;
        if checks == 4 {
            Err(EditorError::new("CANCELLED", "stop"))
        } else {
            Ok(())
        }
    })
    .unwrap_err();
    assert_eq!(error.code, "CANCELLED");
    assert_eq!(checks, 4);
}

#[test]
fn subtitle_ass_multiple_cues_reject_swapped_extra_observations_and_locked_track() {
    let (mut snapshot, mut request) = fixture();
    request
        .content
        .push_str("Dialogue: 0,0:00:04.00,0:00:05.00,Default,,0,0,0,,Second\n");
    let prepared = prepare(&snapshot, &request).unwrap();
    let layers = layers(&prepared);
    let good = observations(&prepared, &layers);
    assert_eq!(layers.len(), 2);
    let mut swapped = good.clone();
    swapped.swap(0, 1);
    request.observations = Some(swapped);
    assert!(finish(&prepared, &request, &layers).is_err());
    let mut extra = good.clone();
    extra.push(good[0].clone());
    request.observations = Some(extra);
    assert!(finish(&prepared, &request, &layers).is_err());
    request.observations = None;
    let mut track = snapshot.project.sequences[0].tracks[0].clone();
    track.id = "locked-text".into();
    track.kind = SceneTrackKind::Text;
    track.locked = true;
    snapshot.project.sequences[0].tracks.push(track);
    request.track = TextTrackTarget::Existing {
        track_id: "locked-text".into(),
    };
    request.content = ass("{\\fnMissing}A");
    assert_eq!(
        prepare(&snapshot, &request).unwrap_err().code,
        "TRACK_LOCKED"
    );
}
