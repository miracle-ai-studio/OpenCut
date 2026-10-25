use super::*;
const STYLE_FORMAT: &str = "Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding";
const DEFAULT: &str = "Default,Lato,24,&H80402010,&H00FFFFFF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,0,0,2,10,20,30,1";
fn document(body: &str) -> String {
    format!(
        "[Script Info]\nScriptType: v4.00+\nPlayResX: 320\nPlayResY: 180\nWrapStyle: 2\n[V4+ Styles]\nFormat: {STYLE_FORMAT}\nStyle: {DEFAULT}\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.25,0:00:03.50,Default,,0,0,0,,{body}\n"
    )
}
#[test]
fn ass_parser_static_export_tags_preserve_units_abgr_and_uniform_text() {
    let ass = document(
        "{\\fnLato\\b700\\i1\\u0\\s1\\fs27.5\\fsp1.5\\fscx125\\fscy75\\frz-30\\an5\\pos(192,78)\\1c&H1133CC&\\1a&H80&\\bord0\\shad0\\q2}Hello, world\\N中文\\nEnd",
    );
    let parsed = parse(&ass).unwrap();
    assert_eq!(
        (parsed.width, parsed.height, parsed.wrap_style),
        (320, 180, 2)
    );
    let cue = &parsed.cues[0];
    assert_eq!(cue.text, "Hello, world\n中文\nEnd");
    assert_eq!(
        cue.timeline_range.start,
        Time {
            ticks: "125".into(),
            timebase: 100
        }
    );
    assert_eq!(
        cue.timeline_range.duration,
        Time {
            ticks: "225".into(),
            timebase: 100
        }
    );
    assert_eq!(cue.position, Some(AssPosition { x: 192.0, y: 78.0 }));
    let s = &cue.style;
    assert_eq!(s.font_family, "Lato");
    assert_eq!(s.font_size_px, 27.5);
    assert_eq!(s.font_weight, FontWeight::Bold);
    assert_eq!(s.font_style, FontStyle::Italic);
    assert_eq!(s.decoration, TextDecoration::LineThrough);
    assert_eq!(
        (
            s.scale_x,
            s.scale_y,
            s.letter_spacing_px,
            s.rotation_degrees,
            s.alignment
        ),
        (1.25, 0.75, 1.5, 30.0, 5)
    );
    assert_eq!(
        (s.color.red, s.color.green, s.color.blue, s.color.alpha),
        (
            204.0 / 255.0,
            51.0 / 255.0,
            17.0 / 255.0,
            1.0 - 128.0 / 255.0
        )
    );
}
#[test]
fn ass_parser_dynamic_event_format_and_zero_margin_inheritance() {
    let ass = document("Text, has, commas")
        .replace(
            "Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text",
            "Style, End, Layer, Start, MarginR, MarginV, Name, MarginL, Effect, Text",
        )
        .replace(
            "0,0:00:01.25,0:00:03.50,Default,,0,0,0,,",
            "Default,0:00:03.50,4,0:00:01.25,9,0,,7,,",
        );
    let parsed = parse(&ass).unwrap();
    let cue = &parsed.cues[0];
    assert_eq!(cue.text, "Text, has, commas");
    assert_eq!(cue.layer, 4);
    assert_eq!(
        cue.margins,
        AssMargins {
            left: 7,
            right: 9,
            vertical: 30
        }
    );
    // Reorder Styles with a complete matching field permutation.
    let ass=document("A").replace(STYLE_FORMAT,"Fontname, Name, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding").replace("Style: Default,Lato,","Style: Lato,Default,");
    assert_eq!(parse(&ass).unwrap().cues[0].style.font_family, "Lato");
}
#[test]
fn ass_parser_soft_break_depends_on_effective_q_and_reset_restores_style() {
    for mode in 0..=3 {
        let ass = document(&format!("{{\\q{mode}}}A\\nB\\NC"));
        let cue = &parse(&ass).unwrap().cues[0];
        assert_eq!(cue.text, if mode == 2 { "A\nB\nC" } else { "A B\nC" });
        assert_eq!(cue.wrap_style, mode);
    }
    let parsed = parse(&document("{\\b700\\i1\\fs99\\alpha&HFF&\\r}A")).unwrap();
    let cue = &parsed.cues[0];
    assert_eq!(cue.style.font_weight, FontWeight::Normal);
    assert_eq!(cue.style.font_style, FontStyle::Normal);
    assert_eq!(cue.style.font_size_px, 24.0);
    assert_eq!(cue.style.color.alpha, 1.0 - 128.0 / 255.0);
    let ass=document("{\\rOther}A").replace("[Events]","Style: Other,Lato,31,&H00FF0000,&H00FFFFFF,&H00000000,&H00000000,-1,-1,-1,0,90,80,-2,15,1,0,0,9,4,5,6,1\n[Events]");
    let parsed = parse(&ass).unwrap();
    assert_eq!(parsed.cues[0].style.decoration, TextDecoration::Underline);
    assert_eq!(parsed.cues[0].style.font_size_px, 31.0);
    assert_eq!(parsed.cues[0].style.alignment, 9);
}
#[test]
fn ass_parser_rejects_unrepresentable_tags_and_inline_runs() {
    for body in [
        "{\\t(0,100,\\fs50)}A",
        "{\\move(0,0,1,1)}A",
        "{\\k20}A",
        "{\\p1}m 0 0 l 1 1",
        "{\\clip(0,0,1,1)}A",
        "{\\frx10}A",
        "{\\bord2}A",
        "{\\shad2}A",
        "{\\u1\\s1}A",
        "A{\\b1}B",
        "{\\fnhttps://font.test/a.ttf}A",
        "{\\pos(1,2)\\pos(3,4)}A",
    ] {
        assert_eq!(
            parse(&document(body)).unwrap_err().code,
            "SUBTITLE_ASS_UNSUPPORTED",
            "{body}"
        );
    }
    for bad in [
        document("A").replace("1,0,0,2,10,20,30,1", "3,0,0,2,10,20,30,1"),
        format!("{}\n[Fonts]\nfontname: Font.ttf\nAAAA", document("A")),
        document("A").replace("WrapStyle: 2", "WrapStyle: 2\nYCbCr Matrix: TV.709"),
    ] {
        assert_eq!(parse(&bad).unwrap_err().code, "SUBTITLE_ASS_UNSUPPORTED");
    }
}
#[test]
fn ass_parser_rejects_ambiguous_missing_or_nonfinite_structure() {
    for ass in [
        document("A").replace("PlayResX: 320\n", ""),
        document("A").replace("0:00:03.50", "0:00:00.50"),
        document("{\\fsNaN}A"),
        document("A").replace(
            "Style: Default,Lato",
            "Style: Default,Lato\nStyle: Default,Lato",
        ),
        document("A").replace("Effect, Text", "Text, Effect"),
        document("A").replace("WrapStyle: 2", "WrapStyle: 4"),
    ] {
        assert!(parse(&ass).is_err());
    }
}
