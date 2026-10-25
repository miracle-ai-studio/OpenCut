use super::*;
use opencut_contracts::font_synthesis::{
    FONT_SYNTHESIS_BINDING_ID_PREFIX, FONT_SYNTHESIS_PROVIDER,
};

fn synthesis_project() -> EditorDocument {
    let mut p = project();
    p.fonts[0].weight = 700;
    p.fonts[0].style = FontStyle::Normal;
    p.fonts[0].origin = ResourceOrigin::Generated {
        provider: FONT_SYNTHESIS_PROVIDER.into(),
        generation_id: format!("{FONT_SYNTHESIS_BINDING_ID_PREFIX}{}", "a".repeat(64)),
    };
    p
}

#[test]
fn unused_synthesis_resource_is_structural_only_and_rejects_wrong_actual_face_or_id() {
    let original = synthesis_project();
    validate_project_scene(&original).unwrap();
    for mutation in 0..7 {
        let mut p = original.clone();
        match mutation {
            0 => p.fonts[0].style = FontStyle::Italic,
            1 => p.fonts[0].weight = 500,
            2 => p.fonts[0].face_index = 1,
            3 | 4 | 5 | 6 => {
                let ResourceOrigin::Generated { generation_id, .. } = &mut p.fonts[0].origin else {
                    unreachable!()
                };
                *generation_id = match mutation {
                    3 => format!("{FONT_SYNTHESIS_BINDING_ID_PREFIX}{}", "A".repeat(64)),
                    4 => format!("{FONT_SYNTHESIS_BINDING_ID_PREFIX}{}", "a".repeat(63)),
                    5 => format!("font-derived-v2_{}", "a".repeat(64)),
                    _ => format!("{FONT_SYNTHESIS_BINDING_ID_PREFIX}{}", "g".repeat(64)),
                };
            }
            _ => unreachable!(),
        }
        rejected(&p, "Font synthesis resource");
    }
}

#[test]
fn synthesis_request_requires_explicit_italic_same_weight_without_relaxing_other_providers() {
    let p = synthesis_project();
    let fonts = p
        .fonts
        .iter()
        .map(|font| (font.id.as_str(), font))
        .collect();
    let locked = FontBinding::Locked {
        font_id: "font".into(),
    };
    font_request(&locked, FontWeight::Bold, FontStyle::Italic, &fonts).unwrap();
    assert!(font_request(&locked, FontWeight::Bold, FontStyle::Normal, &fonts).is_err());
    assert!(font_request(&locked, FontWeight::Normal, FontStyle::Italic, &fonts).is_err());
    let mut p = p;
    p.fonts[0].origin = ResourceOrigin::Generated {
        provider: "other".into(),
        generation_id: "valid".into(),
    };
    let fonts = p
        .fonts
        .iter()
        .map(|font| (font.id.as_str(), font))
        .collect();
    assert!(font_request(&locked, FontWeight::Bold, FontStyle::Italic, &fonts).is_err());
    font_request(&locked, FontWeight::Bold, FontStyle::Normal, &fonts).unwrap();
    p.fonts[0].style = FontStyle::Italic;
    let fonts = p
        .fonts
        .iter()
        .map(|font| (font.id.as_str(), font))
        .collect();
    font_request(&locked, FontWeight::Bold, FontStyle::Italic, &fonts).unwrap();
}

#[test]
fn text_masks_keep_the_authored_italic_request_even_when_disabled() {
    let mut p = synthesis_project();
    let mut mask = Mask {
        id: "mask".into(),
        enabled: false,
        inverted: false,
        shape: LayerMaskShape::Text {
            mask: TextMask {
                content: "AV".into(),
                font: FontBinding::Locked {
                    font_id: "font".into(),
                },
                font_weight: FontWeight::Bold,
                font_style: FontStyle::Italic,
                font_size: FontSize::CanvasPixels { value: 24.0 },
                decoration: TextDecoration::None,
                letter_spacing_px: 0.0,
                line_height_multiplier: 1.2,
                center_x: 0.0,
                center_y: 0.0,
                rotation_degrees: 0.0,
                scale: 1.0,
            },
        },
        feather: MaskFeather::ClassicPixels { value: 0.0 },
        stroke: Stroke {
            color: rgba(0.0, 0.0, 0.0),
            width_px: 0.0,
            align: StrokeAlign::Center,
        },
    };
    first(&mut p)
        .visual
        .as_mut()
        .unwrap()
        .masks
        .push(mask.clone());
    validate_project_scene(&p).unwrap();
    if let LayerMaskShape::Text { mask } = &mut mask.shape {
        mask.font_style = FontStyle::Normal;
    }
    first(&mut p).visual.as_mut().unwrap().masks[0] = mask;
    rejected(&p, "explicit same-weight italic");
}

#[test]
fn captions_reject_synthesis_in_both_layouts_even_outside_the_current_frame() {
    let mut p = synthesis_project();
    p.sequences[0]
        .tracks
        .push(track("caption-track", SceneTrackKind::Caption, vec![]));
    p.sequences[0].captions.push(Caption {
        id: "caption".into(),
        track_id: "caption-track".into(),
        timeline_range: span(t(100, 1), t(1, 1)),
        text: "AV".into(),
        source_anchor: None,
        style: SceneCaptionStyle {
            font: FontBinding::Locked {
                font_id: "font".into(),
            },
            font_size: FontSize::CanvasPixels { value: 24.0 },
            color: rgba(1.0, 1.0, 1.0),
            background_color: None,
            max_width_px: None,
            line_height_px: None,
            text_align: SceneTextAlign::Center,
            layout: CaptionLayoutProfile::PinnedCaptionV4,
        },
    });
    rejected(&p, "CaptionV4 has no requested font style");
    p.sequences[0].captions[0].style.layout = CaptionLayoutProfile::LegacyCaptionV2;
    rejected(&p, "CaptionV4 has no requested font style");
}

#[test]
fn regular_synthesis_is_explicit_same_weight_and_never_synthetic_bold() {
    let mut p = synthesis_project();
    p.fonts[0].weight = 400;
    validate_project_scene(&p).unwrap();
    let fonts = p
        .fonts
        .iter()
        .map(|font| (font.id.as_str(), font))
        .collect();
    let locked = FontBinding::Locked {
        font_id: "font".into(),
    };
    font_request(&locked, FontWeight::Normal, FontStyle::Italic, &fonts).unwrap();
    assert!(font_request(&locked, FontWeight::Bold, FontStyle::Italic, &fonts).is_err());
    assert!(font_request(&locked, FontWeight::Normal, FontStyle::Normal, &fonts).is_err());
}
