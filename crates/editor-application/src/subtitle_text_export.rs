//! Explicit text-track interchange; ASS is not a pixel-preserving project format.
//! Tag semantics: https://aegisub.org/docs/latest/ass_tags/ (an, pos, frz, 1c/1a).
use opencut_api_contracts::subtitle_exchange::SubtitleExportReceipt;
use opencut_contracts::{
    EditorError, Time, scene::*, subtitle_text::SubtitleTextExport, subtitles::SubtitleFormat,
};
use opencut_time::{RationalTime, range_end};
use std::collections::BTreeSet;
fn invalid(s: &str) -> EditorError {
    EditorError::new("SUBTITLE_INVALID", s)
}
fn units(time: &Time, base: i128) -> Result<(i64, bool), EditorError> {
    let t = RationalTime::try_from(time)?;
    if t.ticks() < 0 {
        return Err(invalid("Negative subtitle time"));
    }
    let n = i128::from(t.ticks())
        .checked_mul(base)
        .ok_or_else(|| invalid("Subtitle time overflow"))?;
    let d = i128::from(t.timebase());
    Ok((
        i64::try_from((n + d / 2) / d).map_err(|_| invalid("Subtitle time overflow"))?,
        n % d != 0,
    ))
}
fn clock(t: i64, ass: bool) -> String {
    let b = if ass { 100 } else { 1000 };
    let s = t / b;
    if ass {
        format!("{}:{:02}:{:02}.{:02}", s / 3600, s / 60 % 60, s % 60, t % b)
    } else {
        format!(
            "{:02}:{:02}:{:02},{:03}",
            s / 3600,
            s / 60 % 60,
            s % 60,
            t % b
        )
    }
}
fn ass_text(text: &str) -> Result<String, EditorError> {
    // ASS has no portable escape for literal override braces/backslash sequences.
    // Reject instead of silently editing content or permitting override injection.
    if text.contains(['{', '}', '\\', '\r', '\0'])
        || text.chars().any(|c| c.is_control() && c != '\n')
    {
        return Err(invalid("Text cannot be represented safely in ASS"));
    }
    Ok(text.replace('\n', "\\N"))
}
fn safe_family(s: &str) -> Result<&str, EditorError> {
    if s.is_empty() || s.contains([',', '{', '}', '\\']) || s.chars().any(char::is_control) {
        Err(invalid("Font family cannot be represented safely in ASS"))
    } else {
        Ok(s)
    }
}
fn srgb(v: f64, encoding: ColorEncoding) -> u8 {
    let v = if encoding == ColorEncoding::LinearSrgb {
        if v <= 0.0031308 {
            v * 12.92
        } else {
            1.055 * v.powf(1. / 2.4) - 0.055
        }
    } else {
        v
    };
    (v.clamp(0., 1.) * 255.).round() as u8
}
fn tags(
    project: &EditorDocument,
    text: &Text,
    visual: &VisualStyle,
    track: &SceneTrack,
    size: CanvasSize,
    losses: &mut BTreeSet<String>,
) -> Result<String, EditorError> {
    let family = match &text.font {
        FontBinding::Locked { font_id } => {
            &project
                .fonts
                .iter()
                .find(|f| &f.id == font_id)
                .ok_or_else(|| invalid("Locked font absent"))?
                .family
        }
        FontBinding::UnresolvedFamily { family, .. } => {
            losses.insert("Unresolved font availability is not established by ASS export".into());
            family
        }
    };
    let family = safe_family(family)?;
    let fs = match text.font_size {
        FontSize::CanvasPixels { value } => value,
        FontSize::ClassicHeight90 { value } => value * f64::from(size.height) / 90.,
    };
    if visual.transform.scale_x <= 0.0 || visual.transform.scale_y <= 0.0 {
        losses
            .insert("Negative or zero scale mirroring may not be honored by ASS renderers".into());
    }
    losses.insert("Color and combined opacity are quantized to ASS 8-bit channels".into());
    let c = text.color;
    let alpha = ((1. - c.alpha * visual.opacity * track.opacity) * 255.)
        .round()
        .clamp(0., 255.) as u8;
    let tr = visual.transform;
    let align = match text.align {
        SceneTextAlign::Left => 4,
        SceneTextAlign::Center => 5,
        SceneTextAlign::Right => 6,
    };
    if align != 5 {
        losses.insert("ASS left/right anchor uses canvas position; original measured text-box anchor is not preserved".into());
    }
    if text.background.enabled {
        losses.insert(
            "Text background color, rounded corners, padding and offsets are not represented"
                .into(),
        );
    }
    if text.line_height_multiplier != 1. {
        losses.insert("Text line-height multiplier is not represented".into());
    }
    if !visual.effects.is_empty() || !visual.masks.is_empty() {
        losses.insert("Visual effect and mask stacks are not represented".into());
    }
    if visual.blend_mode != BlendMode::Normal {
        losses.insert("Visual blend mode is not represented".into());
    }
    if visual.profile != VisualProfile::ClassicCanvasV31 {
        losses.insert("Legacy geometry profile is approximated by ASS canvas coordinates".into());
    }
    if (tr.scale_x - tr.scale_y).abs() > 1e-9 && tr.rotation_degrees.to_radians().sin().abs() > 1e-9
    {
        losses.insert("ASS rotates scaled text while Classic scales rotated text; combined nonuniform scale and rotation are not preserved".into());
    }
    losses.insert(
        "ASS font family and face flags do not embed pinned font bytes, license, or CAS identity"
            .into(),
    );
    losses.insert(
        "ASS font metrics, multiline layout and renderer behavior may differ from Classic pixels"
            .into(),
    );
    Ok(format!(
        "{{\\fn{family}\\b{}\\i{}\\u{}\\s{}\\fs{fs}\\fsp{}\\fscx{}\\fscy{}\\frz{}\\an{align}\\pos({},{})\\1c&H{:02X}{:02X}{:02X}&\\1a&H{alpha:02X}&\\bord0\\shad0\\q2}}",
        if text.font_weight == FontWeight::Bold {
            700
        } else {
            400
        },
        u8::from(text.font_style == FontStyle::Italic),
        u8::from(text.decoration == TextDecoration::Underline),
        u8::from(text.decoration == TextDecoration::LineThrough),
        text.letter_spacing_px,
        tr.scale_x * 100.,
        tr.scale_y * 100.,
        -tr.rotation_degrees,
        f64::from(size.width) / 2. + tr.position_x_px,
        f64::from(size.height) / 2. + tr.position_y_px,
        srgb(c.blue, c.encoding),
        srgb(c.green, c.encoding),
        srgb(c.red, c.encoding)
    ))
}
pub fn export(
    snapshot: &EditorSnapshot,
    input: &SubtitleTextExport,
) -> Result<SubtitleExportReceipt, EditorError> {
    if snapshot.project.id != input.project_id || snapshot.revision != input.revision {
        return Err(invalid("Fixed subtitle export scope differs"));
    }
    opencut_editor_core::scene::validate_project_scene(&snapshot.project)?;
    let scene = snapshot
        .project
        .sequences
        .iter()
        .find(|s| s.id == input.sequence_id)
        .ok_or_else(|| invalid("Sequence absent"))?;
    let track = scene
        .tracks
        .iter()
        .find(|t| t.id == input.track_id && t.kind == SceneTrackKind::Text)
        .ok_or_else(|| invalid("Explicit text track absent"))?;
    if track.elements.len() > 5000 {
        return Err(invalid("Too many subtitle cues"));
    }
    let size = match &snapshot.project.settings {
        SettingsProfile::ClassicGlobal { settings } => settings.canvas_size,
        SettingsProfile::LegacyPerScene { formats } => {
            formats
                .iter()
                .find(|f| f.sequence_id == scene.id)
                .ok_or_else(|| invalid("Sequence canvas absent"))?
                .canvas_size
        }
    };
    let mut cues = track.elements.iter().collect::<Vec<_>>();
    cues.sort_by(|a, b| {
        RationalTime::try_from(&a.timeline_range.start)
            .unwrap()
            .cmp_exact(RationalTime::try_from(&b.timeline_range.start).unwrap())
            .then(a.id.cmp(&b.id))
    });
    let ass = input.format == SubtitleFormat::Ass;
    let mut losses = BTreeSet::new();
    let mut content = String::new();
    if ass {
        content = format!(
            "[Script Info]\nScriptType: v4.00+\nPlayResX: {}\nPlayResY: {}\nWrapStyle: 2\nScaledBorderAndShadow: yes\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Arial,24,&H00FFFFFF,&H00FFFFFF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,0,0,5,0,0,0,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n",
            size.width, size.height
        );
    } else {
        losses.insert(
            "SRT does not represent text styles, fonts, transforms or visual stacks".into(),
        );
    }
    losses.insert("Track ownership, element IDs and source spans are not represented".into());
    if !track.enabled {
        losses.insert("Disabled track state is not represented; cues are exported".into());
    }
    for (i, e) in cues.iter().enumerate() {
        let ElementContent::Text { text } = &e.content else {
            return Err(invalid("Non-text element in selected text track"));
        };
        if !e.enabled {
            losses.insert("Disabled element state is not represented; cue is exported".into());
        }
        if !track.channels.is_empty() || !e.channels.is_empty() {
            losses.insert(
                "Animation channels are not represented; stored scalar values are exported".into(),
            );
        }
        let (start, rs) = units(&e.timeline_range.start, if ass { 100 } else { 1000 })?;
        let (end, re) = units(&range_end(&e.timeline_range)?, if ass { 100 } else { 1000 })?;
        if end <= start {
            return Err(invalid("Cue shorter than interchange precision"));
        }
        if rs || re {
            losses.insert("Timing rounded to interchange precision".into());
        }
        if ass {
            let visual = e
                .visual
                .as_ref()
                .ok_or_else(|| invalid("Text visual style absent"))?;
            content.push_str(&format!(
                "Dialogue: 0,{},{},Default,,0,0,0,,{}{}\n",
                clock(start, true),
                clock(end, true),
                tags(&snapshot.project, text, visual, track, size, &mut losses)?,
                ass_text(&text.content)?
            ));
        } else {
            if text.content.contains("\n\n") {
                return Err(invalid(
                    "Blank lines inside SRT cue cannot be represented unambiguously",
                ));
            }
            if text.content.contains(['\r', '\0']) {
                return Err(invalid(
                    "Text contains unsupported subtitle control characters",
                ));
            }
            content.push_str(&format!(
                "{}\n{} --> {}\n{}\n\n",
                i + 1,
                clock(start, false),
                clock(end, false),
                text.content
            ));
        }
        if content.len() > 2 * 1024 * 1024 {
            return Err(invalid("Subtitle export exceeds 2 MiB"));
        }
    }
    Ok(SubtitleExportReceipt {
        project_id: input.project_id.clone(),
        revision: input.revision.clone(),
        sequence_id: input.sequence_id.clone(),
        format: if ass {
            SubtitleFormat::Ass
        } else {
            SubtitleFormat::Srt
        },
        content,
        cue_count: cues.len(),
        losses: losses.into_iter().collect(),
    })
}
#[cfg(test)]
#[path = "subtitle_text_export/tests.rs"]
mod tests;
