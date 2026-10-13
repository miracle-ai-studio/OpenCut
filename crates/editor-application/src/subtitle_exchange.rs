//! Strict, bounded SRT/ASS interchange mapped to ordinary caption transactions.
use opencut_api_contracts::subtitle_exchange::{
    SubtitleExport, SubtitleExportReceipt, SubtitleFormat, SubtitleImport,
};
use opencut_contracts::{
    CaptionCue, CaptionStyle, EditorError, FontResolutionPolicy, Operation, ProjectSnapshot,
    TextAlign, Time, TimeRange, TransactionRequest,
};
use opencut_time::{RationalTime, range_end};
use std::collections::{BTreeMap, BTreeSet};

const MAX_CONTENT: usize = 2 * 1024 * 1024;
const MAX_CUES: usize = 5000;
const MAX_TEXT: usize = 16 * 1024;

fn invalid(message: &str) -> EditorError {
    EditorError::new("SUBTITLE_INVALID", message)
}

fn default_style() -> CaptionStyle {
    CaptionStyle {
        font_family: "Arial".into(),
        font_size: 24.0,
        color: "#ffffff".into(),
        background_color: None,
        font_policy: FontResolutionPolicy::Strict {},
        max_width: None,
        line_height: None,
        text_align: TextAlign::Center,
    }
}

fn sequence<'a>(
    snapshot: &'a ProjectSnapshot,
    id: &str,
) -> Result<&'a opencut_contracts::Sequence, EditorError> {
    snapshot
        .project
        .sequences
        .iter()
        .find(|item| item.id == id)
        .ok_or_else(|| EditorError::new("NOT_FOUND", "Sequence does not exist"))
}

fn parse_clock(input: &str, ass: bool) -> Result<i64, EditorError> {
    let (clock, fraction) = input
        .trim()
        .rsplit_once(if ass { '.' } else { ',' })
        .ok_or_else(|| invalid("Subtitle timestamp needs a fractional part"))?;
    let parts: Vec<_> = clock.split(':').collect();
    if parts.len() != 3
        || fraction.len() != if ass { 2 } else { 3 }
        || !fraction.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(invalid("Subtitle timestamp format is invalid"));
    }
    let numbers = parts
        .iter()
        .map(|part| {
            if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                return Err(invalid("Subtitle timestamp has invalid digits"));
            }
            part.parse::<i64>()
                .map_err(|_| invalid("Subtitle timestamp overflow"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if numbers[1] >= 60 || numbers[2] >= 60 {
        return Err(invalid("Subtitle minute or second exceeds 59"));
    }
    let units = if ass { 100 } else { 1000 };
    numbers[0]
        .checked_mul(3600)
        .and_then(|n| n.checked_add(numbers[1] * 60))
        .and_then(|n| n.checked_add(numbers[2]))
        .and_then(|n| n.checked_mul(units))
        .and_then(|n| n.checked_add(fraction.parse::<i64>().ok()?))
        .and_then(|n| n.checked_mul(1000 / units))
        .ok_or_else(|| invalid("Subtitle timestamp overflow"))
}

fn cue_range(start: i64, end: i64) -> Result<TimeRange, EditorError> {
    if end <= start {
        return Err(invalid("Subtitle end must follow start"));
    }
    Ok(TimeRange {
        start: Time {
            ticks: start.to_string(),
            timebase: 1000,
        },
        duration: Time {
            ticks: (end - start).to_string(),
            timebase: 1000,
        },
    })
}

pub(crate) fn parse_srt(
    content: &str,
    style: &CaptionStyle,
) -> Result<Vec<CaptionCue>, EditorError> {
    let normalized = content
        .trim_start_matches('\u{feff}')
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let mut cues = Vec::new();
    for block in normalized
        .split("\n\n")
        .filter(|part| !part.trim().is_empty())
    {
        let mut lines = block.lines();
        let first = lines.next().ok_or_else(|| invalid("Empty SRT cue"))?;
        let time_line = if first.trim().bytes().all(|b| b.is_ascii_digit()) {
            lines
                .next()
                .ok_or_else(|| invalid("SRT cue is missing time"))?
        } else {
            first
        };
        let (start, end) = time_line
            .split_once(" --> ")
            .ok_or_else(|| invalid("SRT cue needs a start and end"))?;
        let text = lines.collect::<Vec<_>>().join("\n");
        if text.trim().is_empty() || text.len() > MAX_TEXT {
            return Err(invalid("SRT cue text is empty or too large"));
        }
        cues.push(CaptionCue {
            id: String::new(),
            timeline_range: cue_range(parse_clock(start, false)?, parse_clock(end, false)?)?,
            text,
            source_anchor: None,
            style: style.clone(),
        });
        if cues.len() > MAX_CUES {
            return Err(invalid("Too many subtitle cues"));
        }
    }
    if cues.is_empty() {
        return Err(invalid("Subtitle file contains no cues"));
    }
    Ok(cues)
}

fn ass_style(columns: &[String], values: &str) -> Result<(String, CaptionStyle), EditorError> {
    let fields: Vec<_> = values.trim_start().split(',').map(str::trim).collect();
    if fields.len() != columns.len() {
        return Err(invalid("ASS Style has missing fields"));
    }
    let mut result = default_style();
    let mut name = None;
    for (column, value) in columns.iter().zip(fields) {
        match column.as_str() {
            "name" => name = Some(value.to_string()),
            "fontname" if !value.is_empty() => result.font_family = value.into(),
            "fontsize" => {
                result.font_size = value
                    .parse::<f64>()
                    .map_err(|_| invalid("ASS font size is invalid"))?
            }
            "primarycolour" => {
                let hex = value
                    .strip_prefix("&H")
                    .or_else(|| value.strip_prefix("&h"))
                    .ok_or_else(|| invalid("ASS primary colour is invalid"))?;
                let raw = u32::from_str_radix(hex, 16)
                    .map_err(|_| invalid("ASS primary colour is invalid"))?;
                if raw >> 24 != 0 {
                    return Err(invalid("ASS alpha is unsupported"));
                }
                result.color = format!(
                    "#{:02x}{:02x}{:02x}",
                    raw & 255,
                    (raw >> 8) & 255,
                    (raw >> 16) & 255
                );
            }
            "alignment" => {
                result.text_align = match value {
                    "1" => TextAlign::Left,
                    "2" => TextAlign::Center,
                    "3" => TextAlign::Right,
                    _ => return Err(invalid("ASS vertical alignment is unsupported")),
                }
            }
            "secondarycolour" if value.eq_ignore_ascii_case("&H00FFFFFF") => {}
            "outlinecolour" | "backcolour" if value.eq_ignore_ascii_case("&H00000000") => {}
            "bold" | "italic" | "underline" | "strikeout" | "spacing" | "angle" | "outline"
            | "shadow" | "marginl" | "marginr" | "marginv"
                if value == "0" => {}
            "scalex" | "scaley" if value == "100" => {}
            "borderstyle" | "encoding" if value == "1" => {}
            _ => return Err(invalid("ASS style feature cannot be represented")),
        }
    }
    let name = name.ok_or_else(|| invalid("ASS Style lacks Name"))?;
    if name.is_empty()
        || result.font_family.trim().is_empty()
        || !result.font_size.is_finite()
        || result.font_size <= 0.0
    {
        return Err(invalid("ASS Style has invalid font or name"));
    }
    Ok((name, result))
}

pub(crate) fn parse_ass(
    content: &str,
    style: &CaptionStyle,
) -> Result<Vec<CaptionCue>, EditorError> {
    let normalized = content
        .trim_start_matches('\u{feff}')
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let mut in_events = false;
    let mut in_styles = false;
    let mut columns = Vec::<String>::new();
    let mut style_columns = Vec::<String>::new();
    let mut styles = BTreeMap::<String, CaptionStyle>::new();
    let mut cues = Vec::new();
    for line in normalized.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_events = line.eq_ignore_ascii_case("[Events]");
            in_styles = line.eq_ignore_ascii_case("[V4+ Styles]");
            continue;
        }
        if in_styles {
            if let Some(format) = line.strip_prefix("Format:") {
                style_columns = format
                    .split(',')
                    .map(|x| x.trim().to_ascii_lowercase())
                    .collect();
            } else if let Some(values) = line.strip_prefix("Style:") {
                if style_columns.is_empty() {
                    return Err(invalid("ASS Style needs a Format"));
                }
                let (name, parsed) = ass_style(&style_columns, values)?;
                if styles.insert(name, parsed).is_some() {
                    return Err(invalid("Duplicate ASS Style"));
                }
            } else if !line.is_empty() && !line.starts_with(';') {
                return Err(invalid("Unsupported ASS style entry"));
            }
            continue;
        }
        if !in_events {
            continue;
        }
        if let Some(format) = line.strip_prefix("Format:") {
            columns = format
                .split(',')
                .map(|x| x.trim().to_ascii_lowercase())
                .collect();
            if columns.last().map(String::as_str) != Some("text") {
                return Err(invalid("ASS event Text must be the final column"));
            }
        } else if let Some(dialogue) = line.strip_prefix("Dialogue:") {
            if columns.is_empty() {
                return Err(invalid("ASS Dialogue needs an Events Format"));
            }
            let fields: Vec<_> = dialogue.trim_start().splitn(columns.len(), ',').collect();
            if fields.len() != columns.len() {
                return Err(invalid("ASS Dialogue has missing fields"));
            }
            let field = |name: &str| -> Result<&str, EditorError> {
                columns
                    .iter()
                    .position(|item| item == name)
                    .map(|index| fields[index].trim())
                    .ok_or_else(|| invalid("ASS Dialogue lacks a required column"))
            };
            let start = parse_clock(field("start")?, true)?;
            let end = parse_clock(field("end")?, true)?;
            if field("layer")? != "0" {
                return Err(invalid("ASS layers are unsupported"));
            }
            let style_name = field("style")?;
            let cue_style = if styles.is_empty() {
                if style_name != "Default" {
                    return Err(invalid("ASS Style is missing"));
                }
                style.clone()
            } else {
                styles
                    .get(style_name)
                    .ok_or_else(|| invalid("ASS Style is missing"))?
                    .clone()
            };
            for optional in ["effect", "name", "marginl", "marginr", "marginv"] {
                if let Some(index) = columns.iter().position(|item| item == optional) {
                    let value = fields[index].trim();
                    if !value.is_empty() && value != "0" && value != "0000" {
                        return Err(invalid(
                            "ASS actor, effect, or margin cannot be represented",
                        ));
                    }
                }
            }
            let raw = field("text")?;
            if raw.contains('{') || raw.contains('}') || raw.contains("\\h") {
                return Err(invalid("ASS override tags and hard spaces are unsupported"));
            }
            let text = raw.replace("\\N", "\n").replace("\\n", "\n");
            if text.trim().is_empty() || text.len() > MAX_TEXT {
                return Err(invalid("ASS cue text is empty or too large"));
            }
            cues.push(CaptionCue {
                id: String::new(),
                timeline_range: cue_range(start, end)?,
                text,
                source_anchor: None,
                style: cue_style,
            });
            if cues.len() > MAX_CUES {
                return Err(invalid("Too many subtitle cues"));
            }
        } else if line.starts_with("Comment:") {
            continue;
        } else if !line.is_empty() && !line.starts_with(";") {
            return Err(invalid("Unsupported ASS event"));
        }
    }
    if cues.is_empty() {
        return Err(invalid("Subtitle file contains no cues"));
    }
    Ok(cues)
}

/// Returns the exact transaction to commit through the ordinary CAS/idempotency path.
pub fn import_transaction(
    snapshot: &ProjectSnapshot,
    input: &SubtitleImport,
) -> Result<TransactionRequest, EditorError> {
    if snapshot.project.id != input.project_id || snapshot.revision != input.expected_revision {
        return Err(EditorError::new(
            "REVISION_CONFLICT",
            "Subtitle import revision changed",
        ));
    }
    sequence(snapshot, &input.sequence_id)?;
    if input.content.len() > MAX_CONTENT || input.idempotency_key.is_empty() {
        return Err(invalid(
            "Subtitle input is too large or lacks an idempotency key",
        ));
    }
    let style = input.style.clone().unwrap_or_else(default_style);
    let mut cues = match input.format {
        SubtitleFormat::Srt => parse_srt(&input.content, &style)?,
        SubtitleFormat::Ass => parse_ass(&input.content, &style)?,
    };
    let operations = cues
        .iter_mut()
        .enumerate()
        .map(|(index, cue)| {
            cue.id = crate::content_id(
                "cue",
                format!(
                    "subtitle:{}:{}:{}",
                    input.project_id, input.idempotency_key, index
                )
                .as_bytes(),
            );
            Operation::InsertCaption {
                sequence_id: input.sequence_id.clone(),
                cue: cue.clone(),
            }
        })
        .collect();
    Ok(TransactionRequest {
        expected_revision: input.expected_revision.clone(),
        idempotency_key: input.idempotency_key.clone(),
        label: "Import subtitles".into(),
        operations,
    })
}

fn rounded_units(time: &Time, units: i128) -> Result<(i64, bool), EditorError> {
    let t = RationalTime::try_from(time)?;
    if t.ticks() < 0 {
        return Err(invalid("Negative subtitle time cannot be exported"));
    }
    let numerator = (t.ticks() as i128)
        .checked_mul(units)
        .ok_or_else(|| invalid("Subtitle time overflow"))?;
    let denominator = t.timebase() as i128;
    let value = (numerator + denominator / 2) / denominator;
    Ok((
        i64::try_from(value).map_err(|_| invalid("Subtitle time overflow"))?,
        numerator % denominator != 0,
    ))
}

fn clock(units: i64, ass: bool) -> String {
    let base = if ass { 100 } else { 1000 };
    let second = units / base;
    if ass {
        format!(
            "{}:{:02}:{:02}.{:02}",
            second / 3600,
            (second / 60) % 60,
            second % 60,
            units % base
        )
    } else {
        format!(
            "{:02}:{:02}:{:02},{:03}",
            second / 3600,
            (second / 60) % 60,
            second % 60,
            units % base
        )
    }
}

fn ass_style_row(
    name: &str,
    style: &CaptionStyle,
    losses: &mut BTreeSet<String>,
) -> Result<String, EditorError> {
    if style
        .font_family
        .chars()
        .any(|c| matches!(c, ',' | '\n' | '\r'))
        || style.font_family.trim().is_empty()
        || !style.font_size.is_finite()
        || style.font_size <= 0.0
    {
        return Err(invalid("Caption font cannot be encoded as ASS Style"));
    }
    let color = style
        .color
        .strip_prefix('#')
        .ok_or_else(|| invalid("Caption colour is not #RRGGBB"))?;
    if color.len() != 6 {
        return Err(invalid("Caption colour is not #RRGGBB"));
    }
    let rgb =
        u32::from_str_radix(color, 16).map_err(|_| invalid("Caption colour is not #RRGGBB"))?;
    let bgr = ((rgb & 255) << 16) | (rgb & 0x00ff00) | (rgb >> 16);
    let align = match style.text_align {
        TextAlign::Left => 1,
        TextAlign::Center => 2,
        TextAlign::Right => 3,
    };
    if style.background_color.is_some() {
        losses.insert("caption background colour is not represented by ASS".into());
    }
    if style.max_width.is_some() || style.line_height.is_some() {
        losses.insert("caption layout bounds are not represented by ASS".into());
    }
    if !matches!(style.font_policy, FontResolutionPolicy::Strict {}) {
        losses.insert("font fallback policy is not represented by ASS".into());
    }
    Ok(format!(
        "Style: {name},{},{},&H00{bgr:06X},&H00FFFFFF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,0,0,{align},0,0,0,1\n",
        style.font_family, style.font_size
    ))
}

pub fn export_subtitles(
    snapshot: &ProjectSnapshot,
    input: &SubtitleExport,
) -> Result<SubtitleExportReceipt, EditorError> {
    if snapshot.project.id != input.project_id || snapshot.revision != input.revision {
        return Err(EditorError::new(
            "REVISION_CONFLICT",
            "Subtitle export revision changed",
        ));
    }
    let sequence = sequence(snapshot, &input.sequence_id)?;
    if sequence.caption_cues.len() > MAX_CUES {
        return Err(invalid("Too many subtitle cues"));
    }
    let mut cues = sequence.caption_cues.iter().collect::<Vec<_>>();
    cues.sort_by(|a, b| {
        let a = RationalTime::try_from(&a.timeline_range.start).unwrap();
        let b = RationalTime::try_from(&b.timeline_range.start).unwrap();
        a.cmp_exact(b).then(a.ticks().cmp(&b.ticks()))
    });
    let mut losses = BTreeSet::new();
    let mut styles = Vec::<CaptionStyle>::new();
    let mut content = String::new();
    if input.format == SubtitleFormat::Ass {
        content.push_str("[Script Info]\nScriptType: v4.00+\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n");
        for cue in &cues {
            if !styles.contains(&cue.style) {
                styles.push(cue.style.clone());
            }
        }
        for (index, style) in styles.iter().enumerate() {
            content.push_str(&ass_style_row(
                &format!("CueStyle{index}"),
                style,
                &mut losses,
            )?);
        }
        content.push_str("\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n");
    }
    for (index, cue) in cues.iter().enumerate() {
        let end = range_end(&cue.timeline_range)?;
        let (start, rounded_start) = rounded_units(
            &cue.timeline_range.start,
            if input.format == SubtitleFormat::Ass {
                100
            } else {
                1000
            },
        )?;
        let (end, rounded_end) = rounded_units(
            &end,
            if input.format == SubtitleFormat::Ass {
                100
            } else {
                1000
            },
        )?;
        if end <= start {
            return Err(invalid(
                "Subtitle cue is shorter than interchange precision",
            ));
        }
        if rounded_start || rounded_end {
            losses.insert("timing rounded to interchange precision".to_string());
        }
        if cue.source_anchor.is_some() {
            losses.insert("source anchors are not represented".to_string());
        }
        if input.format == SubtitleFormat::Srt {
            losses.insert("caption styles are not represented by SRT".to_string());
            content.push_str(&format!(
                "{}\n{} --> {}\n{}\n\n",
                index + 1,
                clock(start, false),
                clock(end, false),
                cue.text
            ));
        } else {
            if cue.text.contains('{')
                || cue.text.contains('}')
                || cue.text.contains("\\N")
                || cue.text.contains("\\n")
            {
                return Err(invalid(
                    "Caption text contains ASS syntax that cannot be escaped safely",
                ));
            }
            let style_id = styles
                .iter()
                .position(|style| style == &cue.style)
                .ok_or_else(|| invalid("ASS Style inventory is incomplete"))?;
            content.push_str(&format!(
                "Dialogue: 0,{},{},CueStyle{style_id},,0,0,0,,{}\n",
                clock(start, true),
                clock(end, true),
                cue.text.replace('\n', "\\N")
            ));
        }
    }
    if content.len() > MAX_CONTENT {
        return Err(invalid("Exported subtitle file exceeds size limit"));
    }
    Ok(SubtitleExportReceipt {
        project_id: input.project_id.clone(),
        revision: input.revision.clone(),
        sequence_id: input.sequence_id.clone(),
        format: input.format,
        content,
        cue_count: cues.len(),
        losses: losses.into_iter().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencut_contracts::{CreateProject, FrameRate};

    fn snapshot() -> ProjectSnapshot {
        let project = opencut_editor_core::create_project(&CreateProject {
            id: "p".into(),
            name: "P".into(),
            sequence_id: "main".into(),
            width: 64,
            height: 48,
            fps: FrameRate {
                numerator: 24,
                denominator: 1,
            },
        })
        .unwrap();
        ProjectSnapshot {
            project,
            revision: "rev".into(),
        }
    }

    #[test]
    fn srt_import_and_export_preserve_text_and_millisecond_times() {
        let source = "1\r\n00:00:01,250 --> 00:00:02,500\r\n中文\r\nsecond line\r\n";
        let input = SubtitleImport {
            project_id: "p".into(),
            expected_revision: "rev".into(),
            sequence_id: "main".into(),
            idempotency_key: "srt".into(),
            format: SubtitleFormat::Srt,
            content: source.into(),
            style: None,
        };
        let transaction = import_transaction(&snapshot(), &input).unwrap();
        assert_eq!(transaction.operations.len(), 1);
        let mut project = snapshot();
        project.project.sequences[0].caption_cues = transaction
            .operations
            .into_iter()
            .map(|op| {
                if let Operation::InsertCaption { cue, .. } = op {
                    cue
                } else {
                    unreachable!()
                }
            })
            .collect();
        let result = export_subtitles(
            &project,
            &SubtitleExport {
                project_id: "p".into(),
                revision: "rev".into(),
                sequence_id: "main".into(),
                format: SubtitleFormat::Srt,
            },
        )
        .unwrap();
        assert!(
            result
                .content
                .contains("00:00:01,250 --> 00:00:02,500\n中文\nsecond line")
        );
        assert!(result.losses.iter().any(|loss| loss.contains("styles")));
    }

    #[test]
    fn ass_plain_dialogue_import_rejects_override_tags() {
        let mut input = SubtitleImport {project_id:"p".into(),expected_revision:"rev".into(),
            sequence_id:"main".into(),idempotency_key:"ass".into(),format:SubtitleFormat::Ass,
            content:"[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.25,0:00:02.50,Default,,0,0,0,,Hello\\N世界".into(),style:None};
        assert_eq!(
            import_transaction(&snapshot(), &input)
                .unwrap()
                .operations
                .len(),
            1
        );
        input.content.push_str("{\\pos(1,2)}");
        assert_eq!(
            import_transaction(&snapshot(), &input).unwrap_err().code,
            "SUBTITLE_INVALID"
        );
    }

    #[test]
    fn ass_export_import_preserves_supported_font_color_and_alignment() {
        let mut original = snapshot();
        let mut style = default_style();
        style.font_family = "PingFang SC".into();
        style.font_size = 31.0;
        style.color = "#12abef".into();
        style.text_align = TextAlign::Right;
        original.project.sequences[0].caption_cues.push(CaptionCue {
            id: "cue".into(),
            timeline_range: cue_range(1250, 2500).unwrap(),
            text: "你好\n世界".into(),
            source_anchor: None,
            style: style.clone(),
        });
        let export = export_subtitles(
            &original,
            &SubtitleExport {
                project_id: "p".into(),
                revision: "rev".into(),
                sequence_id: "main".into(),
                format: SubtitleFormat::Ass,
            },
        )
        .unwrap();
        assert!(export.losses.is_empty(), "{:?}", export.losses);
        let transaction = import_transaction(
            &snapshot(),
            &SubtitleImport {
                project_id: "p".into(),
                expected_revision: "rev".into(),
                sequence_id: "main".into(),
                idempotency_key: "roundtrip".into(),
                format: SubtitleFormat::Ass,
                content: export.content,
                style: None,
            },
        )
        .unwrap();
        let Operation::InsertCaption { cue, .. } = &transaction.operations[0] else {
            panic!()
        };
        assert_eq!(cue.style, style);
        assert_eq!(cue.text, "你好\n世界");
        assert_eq!(
            cue.timeline_range,
            original.project.sequences[0].caption_cues[0].timeline_range
        );
    }
}
