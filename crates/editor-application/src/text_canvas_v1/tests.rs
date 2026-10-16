use super::*;
use opencut_contracts::{CreateProject, FrameRate, Time, project_v4::*};
use serde_json::json;

pub(crate) fn fixture() -> (ProjectSnapshotV4, TextCanvasPlanRequestV1) {
    let mut project = crate::project_v4::new_document(&CreateProject {
        id: "p".into(),
        name: "Canvas text".into(),
        sequence_id: "s".into(),
        width: 320,
        height: 180,
        fps: FrameRate {
            numerator: 24,
            denominator: 1,
        },
    })
    .unwrap();
    project.fonts.push(serde_json::from_value(json!({
        "id":"f", "family":"Fixed face", "weight":400,"style":"normal","face_index":0,
        "content":{"id":format!("av_{}", "a".repeat(64)),"content_hash":"a".repeat(64),"size":100},
        "origin":{"kind":"legacy_locator","uri":"/private/font.ttf"},"license":null
    })).unwrap());
    project.sequences[0].tracks.push(serde_json::from_value(json!({
        "id":"titles","name":"Titles","kind":"text","enabled":true,"locked":false,"muted":false,"opacity":1.0,
        "gain":{"unit":"decibels","value":0.0},"channels":[],"elements":[{
            "id":"e","name":"Title","enabled":true,
            "timeline_range":{"start":{"ticks":"0","timebase":1},"duration":{"ticks":"5","timebase":1}},
            "source_span":{"profile":"classic_trimmed","trim_start":{"ticks":"0","timebase":1},"trim_end":{"ticks":"0","timebase":1},"source_duration":null},
            "channels":[], "visual":{"profile":"classic_canvas_v31","opacity":1.0,"blend_mode":"normal","effects":[],"masks":[],
                "transform":{"position_x_px":17.0,"position_y_px":-20.0,"scale_x":2.0,"scale_y":0.5,"rotation_degrees":90.0}},
            "content":{"kind":"text","text":{"content":"AV To\n中文编辑","font":{"kind":"locked","font_id":"f"},
                "font_weight":"normal","font_style":"normal","decoration":"none","align":"center",
                "font_size":{"unit":"classic_height90","value":15.0},
                "color":{"encoding":"srgb","red":1.0,"green":1.0,"blue":1.0,"alpha":1.0},
                "letter_spacing_px":0.0,"line_height_multiplier":1.2,
                "background":{"enabled":false,"color":{"encoding":"srgb","red":0.0,"green":0.0,"blue":0.0,"alpha":1.0},
                    "corner_radius_percent":0.0,"padding_x_at_size15":30.0,"padding_y_at_size15":42.0,"offset_x_px":0.0,"offset_y_px":0.0}}}
        }]
    })).unwrap());
    (
        ProjectSnapshotV4 {
            project,
            revision: "r".into(),
        },
        TextCanvasPlanRequestV1 {
            project_id: "p".into(),
            revision: "r".into(),
            sequence_id: "s".into(),
            element_id: "e".into(),
            time: Time {
                ticks: "0".into(),
                timebase: 1,
            },
        },
    )
}
#[test]
fn sampled_canvas_plan_retains_renderer_size_and_noncommuting_transform() {
    let (snapshot, input) = fixture();
    let before = snapshot.clone();
    let prepared = prepare_canvas_plan_v1(&snapshot, &input).unwrap();
    let text = &prepared.texts()[0].text;
    assert_eq!(text.font_size_px, 30.0);
    assert_eq!(text.line_height_px, 36.0);
    assert_eq!(text.transform.center_x, 177.0);
    assert_eq!(text.transform.center_y, 70.0);
    assert_eq!(text.transform.scale_x, 2.0);
    assert_eq!(text.transform.scale_y, 0.5);
    assert_eq!(text.transform.rotation_radians, std::f64::consts::FRAC_PI_2);
    assert_eq!(snapshot, before);
}
#[test]
fn canvas_plan_rejects_wrong_fixed_revision_or_project() {
    let (snapshot, input) = fixture();
    for (project_id, revision) in [("other", "r"), ("p", "other")] {
        let request = TextCanvasPlanRequestV1 {
            project_id: project_id.into(),
            revision: revision.into(),
            ..input.clone()
        };
        assert_eq!(
            prepare_canvas_plan_v1(&snapshot, &request)
                .unwrap_err()
                .code,
            "INVALID_ARGUMENT"
        );
    }
}
#[test]
fn canvas_plan_rejects_locked_and_inactive_targets() {
    let (mut snapshot, mut input) = fixture();
    snapshot.project.sequences[0]
        .tracks
        .last_mut()
        .unwrap()
        .locked = true;
    assert_eq!(
        prepare_canvas_plan_v1(&snapshot, &input).unwrap_err().code,
        "TRACK_LOCKED"
    );
    snapshot.project.sequences[0]
        .tracks
        .last_mut()
        .unwrap()
        .locked = false;
    input.time.ticks = "5".into();
    assert_eq!(
        prepare_canvas_plan_v1(&snapshot, &input).unwrap_err().code,
        "NOT_FOUND"
    );
}

#[test]
fn canvas_text_remains_editable_beside_an_active_caption_without_rewriting_source() {
    let (mut snapshot, input) = fixture();
    let mut track = snapshot.project.sequences[0].tracks.last().unwrap().clone();
    track.id = "captions".into();
    track.name = "Captions".into();
    track.kind = TrackKindV4::Caption;
    track.elements.clear();
    snapshot.project.sequences[0].tracks.push(track);
    snapshot.project.sequences[0].captions.push(CaptionV4 {
        id: "caption".into(),
        track_id: "captions".into(),
        timeline_range: opencut_contracts::TimeRange {
            start: Time {
                ticks: "0".into(),
                timebase: 1,
            },
            duration: Time {
                ticks: "5".into(),
                timebase: 1,
            },
        },
        text: "Same scene subtitle".into(),
        source_anchor: None,
        style: CaptionStyleV4 {
            font: FontBindingV4::Locked {
                font_id: "f".into(),
            },
            font_size: FontSizeV4::ClassicHeight90 { value: 5.0 },
            color: ColorV4 {
                encoding: ColorEncodingV4::Srgb,
                red: 1.0,
                green: 1.0,
                blue: 1.0,
                alpha: 1.0,
            },
            background_color: None,
            max_width_px: None,
            line_height_px: None,
            text_align: TextAlignV4::Center,
            layout: CaptionLayoutProfileV4::PinnedCaptionV4,
        },
    });
    let before = snapshot.clone();
    assert!(
        crate::project_v4_text_frame_jobs::prepare(&snapshot, &input.sequence_id, &input.time)
            .is_err()
    );
    let prepared = prepare_canvas_plan_v1(&snapshot, &input).unwrap();
    assert_eq!(prepared.identity().project_id, "p");
    assert_eq!(prepared.identity().revision, "r");
    assert_eq!(prepared.identity().sequence_id, "s");
    assert_eq!(prepared.identity().time, input.time);
    assert_eq!(prepared.texts().len(), 1);
    assert_eq!(prepared.texts()[0].identity.element_id, "e");
    assert_eq!(prepared.graph().layers.len(), 1);
    assert!(prepared.media().layers().is_empty());
    assert_eq!(prepared.graph().fonts.len(), 1);
    assert_eq!(snapshot, before);
}

#[test]
fn canvas_observes_intrinsic_glyph_layout_before_blend_and_blur_without_changing_source() {
    let (mut snapshot, input) = fixture();
    let style = snapshot.project.sequences[0]
        .tracks
        .last_mut()
        .unwrap()
        .elements[0]
        .visual
        .as_mut()
        .unwrap();
    style.blend_mode = BlendModeV4::Multiply;
    style.effects.push(EffectV4 {
        id: "blur".into(),
        enabled: true,
        effect: EffectKindV4::ClassicBlur { intensity: 25.0 },
    });
    let before = snapshot.clone();
    let prepared = prepare_canvas_plan_v1(&snapshot, &input).unwrap();
    let transform = prepared.texts()[0].text.transform;
    assert_eq!(transform.scale_x, 2.0);
    assert_eq!(transform.scale_y, 0.5);
    assert_eq!(snapshot, before);
}
