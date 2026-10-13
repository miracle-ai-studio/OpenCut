use super::*;
use opencut_contracts::{CreateProject, FontResolutionPolicy, FrameRate, TimeRange};
fn t(n: i64) -> Time {
    Time {
        ticks: n.to_string(),
        timebase: 120000,
    }
}
fn fixture() -> (ProjectSnapshotV4, SubtitleTextExportV1) {
    let mut p = crate::project_v4::new_document(&CreateProject {
        id: "p".into(),
        name: "Export".into(),
        sequence_id: "s".into(),
        width: 320,
        height: 180,
        fps: FrameRate {
            numerator: 24,
            denominator: 1,
        },
    })
    .unwrap();
    let color = ColorV4 {
        encoding: ColorEncodingV4::Srgb,
        red: 1.,
        green: 0.,
        blue: 0.,
        alpha: 0.5,
    };
    let text = TextV4 {
        content: "Hello\n世界".into(),
        font: FontBindingV4::UnresolvedFamily {
            family: "Lato".into(),
            policy: FontResolutionPolicy::Strict {},
        },
        font_weight: FontWeightV4::Bold,
        font_style: FontStyleV4::Italic,
        decoration: TextDecorationV4::Underline,
        align: TextAlignV4::Center,
        font_size: FontSizeV4::ClassicHeight90 { value: 15. },
        color,
        letter_spacing_px: 2.,
        line_height_multiplier: 1.2,
        background: TextBackgroundV4 {
            enabled: true,
            color,
            corner_radius_percent: 20.,
            padding_x_at_size15: 10.,
            padding_y_at_size15: 10.,
            offset_x_px: 0.,
            offset_y_px: 0.,
        },
    };
    let e = TimelineElementV4 {
        id: "cue".into(),
        name: "Subtitle".into(),
        timeline_range: TimeRange {
            start: t(120001),
            duration: t(120000),
        },
        source_span: ElementSourceSpanV4::ClassicTrimmed {
            trim_start: t(0),
            trim_end: t(0),
            source_duration: None,
        },
        enabled: true,
        content: ElementContentV4::Text { text },
        channels: vec![],
        visual: Some(VisualStyleV4 {
            profile: VisualProfileV4::ClassicCanvasV31,
            transform: TransformV4 {
                position_x_px: 32.,
                position_y_px: -12.,
                scale_x: 1.25,
                scale_y: 0.75,
                rotation_degrees: 30.,
            },
            opacity: 1.,
            blend_mode: BlendModeV4::Normal,
            effects: vec![],
            masks: vec![],
        }),
    };
    let mut track = p.sequences[0].tracks[0].clone();
    track.id = "subs".into();
    track.kind = TrackKindV4::Text;
    track.elements = vec![e];
    p.sequences[0].tracks.push(track.clone());
    track.id = "titles".into();
    track.elements[0].id = "title".into();
    if let ElementContentV4::Text { text } = &mut track.elements[0].content {
        text.content = "Excluded title".into();
    }
    p.sequences[0].tracks.push(track);
    (
        ProjectSnapshotV4 {
            project: p,
            revision: "r".into(),
        },
        SubtitleTextExportV1 {
            project_id: "p".into(),
            revision: "r".into(),
            sequence_id: "s".into(),
            track_id: "subs".into(),
            format: SubtitleFormatV4::Ass,
        },
    )
}
#[test]
fn explicit_track_ass_style_and_visible_losses() {
    let (s, i) = fixture();
    let out = export(&s, &i).unwrap();
    assert_eq!(out.cue_count, 1);
    assert!(!out.content.contains("Excluded title"));
    for value in [
        "PlayResX: 320",
        "PlayResY: 180",
        r"\fnLato\b700\i1\u1",
        r"\fs30\fsp2\fscx125\fscy75\frz-30\an5\pos(192,78)",
        r"\1c&H0000FF&\1a&H80&",
        r"Hello\N世界",
    ] {
        assert!(out.content.contains(value), "{value}: {}", out.content);
    }
    for loss in [
        "Timing rounded",
        "background",
        "line-height",
        "font bytes",
        "renderer behavior",
    ] {
        assert!(out.losses.iter().any(|s| s.contains(loss)), "{loss}");
    }
}
#[test]
fn srt_preserves_body_and_precision() {
    let (s, mut i) = fixture();
    i.format = SubtitleFormatV4::Srt;
    let out = export(&s, &i).unwrap();
    assert!(
        out.content
            .contains("00:00:01,000 --> 00:00:02,000\nHello\n世界")
    );
    assert!(out.losses.iter().any(|s| s.contains("Timing rounded")));
}
#[test]
fn scope_track_and_override_injection_refused() {
    let (mut s, mut i) = fixture();
    i.revision = "other".into();
    assert!(export(&s, &i).is_err());
    i.revision = "r".into();
    i.track_id = s.project.sequences[0].main_track_id.clone();
    assert!(export(&s, &i).is_err());
    i.track_id = "subs".into();
    if let ElementContentV4::Text { text } =
        &mut s.project.sequences[0].tracks[1].elements[0].content
    {
        text.content = r"{\pos(0,0)}Attack".into();
    }
    assert!(export(&s, &i).is_err());
}
#[test]
fn exact_clock_and_unsafe_family() {
    assert_eq!(
        units(
            &Time {
                ticks: "1".into(),
                timebase: 3
            },
            100
        )
        .unwrap(),
        (33, true)
    );
    assert_eq!(clock(366123, true), "1:01:01.23");
    assert!(safe_family("Lato\\pos(0,0)").is_err());
    assert!(ass_text(r"literal\N").is_err());
}
