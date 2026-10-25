//! Pure static ASS parser. Unsupported rendering semantics are never stripped.
//! Tag semantics: https://aegisub.org/docs/latest/ass_tags/ .
use opencut_contracts::{EditorError, Time, TimeRange, scene::*};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ParsedAssDocument {
    pub width: u32,
    pub height: u32,
    pub wrap_style: u8,
    pub cues: Vec<ParsedAssCue>,
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ParsedAssCue {
    pub timeline_range: TimeRange,
    pub text: String,
    pub style: ParsedAssStyle,
    pub position: Option<AssPosition>,
    pub margins: AssMargins,
    pub wrap_style: u8,
    pub layer: i32,
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ParsedAssStyle {
    pub font_family: String,
    pub font_size_px: f64,
    pub font_weight: FontWeight,
    pub font_style: FontStyle,
    pub decoration: TextDecoration,
    pub color: Color,
    pub scale_x: f64,
    pub scale_y: f64,
    pub letter_spacing_px: f64,
    pub rotation_degrees: f64,
    pub alignment: u8,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct AssPosition {
    pub x: f64,
    pub y: f64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct AssMargins {
    pub left: u32,
    pub right: u32,
    pub vertical: u32,
}
#[derive(Clone, Debug)]
struct Style {
    value: ParsedAssStyle,
    margins: AssMargins,
    underline: bool,
    strike: bool,
    border: u8,
    outline: f64,
    shadow: f64,
}
fn invalid(s: impl Into<String>) -> EditorError {
    EditorError::new("INVALID_SUBTITLE_ASS_PLAN", s)
}
fn unsupported(s: impl Into<String>) -> EditorError {
    EditorError::new("SUBTITLE_ASS_UNSUPPORTED", s)
}
fn number(s: &str) -> Result<f64, EditorError> {
    let n = s
        .trim()
        .parse::<f64>()
        .map_err(|_| invalid("Invalid ASS number"))?;
    if !n.is_finite() || n.abs() > 1_000_000.0 {
        return Err(invalid("ASS numeric value exceeds bounds"));
    }
    Ok(n)
}
fn positive(s: &str) -> Result<f64, EditorError> {
    let n = number(s)?;
    if n <= 0.0 {
        return Err(invalid("ASS size/scale must be positive"));
    }
    Ok(n)
}
fn uint(s: &str) -> Result<u32, EditorError> {
    s.trim()
        .parse()
        .map_err(|_| invalid("Invalid ASS unsigned integer"))
}
fn flag(s: &str) -> Result<bool, EditorError> {
    match s.trim() {
        "0" => Ok(false),
        "-1" | "1" => Ok(true),
        _ => Err(unsupported("ASS boolean value is not representable")),
    }
}
fn weight(s: &str) -> Result<FontWeight, EditorError> {
    match s.trim() {
        "0" | "400" => Ok(FontWeight::Normal),
        "-1" | "1" | "700" => Ok(FontWeight::Bold),
        _ => Err(unsupported(
            "ASS font weight requires a represented 400 or 700 face",
        )),
    }
}
fn alignment(s: &str) -> Result<u8, EditorError> {
    let n = uint(s)?;
    if !(1..=9).contains(&n) {
        return Err(invalid("ASS alignment must be 1..9"));
    }
    Ok(n as u8)
}
fn wrap(s: &str) -> Result<u8, EditorError> {
    let n = uint(s)?;
    if n > 3 {
        return Err(invalid("ASS WrapStyle must be 0..3"));
    }
    Ok(n as u8)
}
fn family(s: &str) -> Result<String, EditorError> {
    let s = s.trim();
    if s.is_empty()
        || s.starts_with('@')
        || s.len() > 1024
        || s.chars().any(char::is_control)
        || s.contains(['/', '\\', ':', '{', '}'])
    {
        return Err(unsupported(
            "ASS font family must be a horizontal face name, not a vertical face, embedded source, path or URL",
        ));
    }
    Ok(s.into())
}
fn hex(s: &str, max: usize) -> Result<u32, EditorError> {
    let s = s.trim();
    let s = s
        .strip_prefix("&H")
        .or_else(|| s.strip_prefix("&h"))
        .ok_or_else(|| invalid("ASS color requires &H hexadecimal syntax"))?;
    let s = s.strip_suffix('&').unwrap_or(s);
    if s.is_empty() || s.len() > max || !s.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(invalid("Invalid ASS hexadecimal color"));
    }
    u32::from_str_radix(s, 16).map_err(|_| invalid("ASS color overflow"))
}
fn color(s: &str) -> Result<Color, EditorError> {
    let n = hex(s, 8)?;
    Ok(Color {
        encoding: ColorEncoding::Srgb,
        red: f64::from(n & 255) / 255.0,
        green: f64::from((n >> 8) & 255) / 255.0,
        blue: f64::from((n >> 16) & 255) / 255.0,
        alpha: 1.0 - f64::from((n >> 24) & 255) / 255.0,
    })
}
fn time(s: &str) -> Result<i64, EditorError> {
    let p = s.trim().split(':').collect::<Vec<_>>();
    if p.len() != 3 {
        return Err(invalid("ASS time requires h:mm:ss.cc"));
    }
    let h = i64::from(uint(p[0])?);
    let m = uint(p[1])?;
    let (seconds, centis) = p[2]
        .split_once('.')
        .ok_or_else(|| invalid("ASS time lacks centiseconds"))?;
    let sec = uint(seconds)?;
    let cs = uint(centis)?;
    if m >= 60 || sec >= 60 || centis.len() != 2 || cs >= 100 {
        return Err(invalid("Invalid ASS clock"));
    }
    h.checked_mul(360000)
        .and_then(|n| n.checked_add(i64::from(m) * 6000 + i64::from(sec) * 100 + i64::from(cs)))
        .ok_or_else(|| invalid("ASS time overflow"))
}
fn format(s: &str) -> Result<Vec<String>, EditorError> {
    let fields = s
        .split(',')
        .map(|s| s.trim().to_ascii_lowercase())
        .collect::<Vec<_>>();
    let set = fields.iter().collect::<BTreeSet<_>>();
    if fields.is_empty() || set.len() != fields.len() || fields.iter().any(|s| s.is_empty()) {
        return Err(invalid("ASS Format contains empty/duplicate fields"));
    }
    Ok(fields)
}
fn row<'a>(
    fields: &[String],
    s: &'a str,
    text: bool,
) -> Result<BTreeMap<String, &'a str>, EditorError> {
    if text && fields.last().is_none_or(|s| s != "text") {
        return Err(unsupported("ASS Events Text must be the last Format field"));
    }
    let values = if text {
        s.splitn(fields.len(), ',').collect::<Vec<_>>()
    } else {
        s.split(',').collect::<Vec<_>>()
    };
    if values.len() != fields.len() {
        return Err(invalid("ASS row differs from declared Format"));
    }
    Ok(fields.iter().cloned().zip(values).collect())
}
fn get<'a>(row: &BTreeMap<String, &'a str>, key: &str) -> Result<&'a str, EditorError> {
    row.get(key)
        .copied()
        .ok_or_else(|| invalid(format!("ASS Format lacks {key}")))
}
fn check_fields(fields: &[String], allowed: &[&str]) -> Result<(), EditorError> {
    for field in fields {
        if !allowed.contains(&field.as_str()) {
            return Err(unsupported(format!("Unsupported ASS Format field {field}")));
        }
    }
    Ok(())
}
fn style(row: &BTreeMap<String, &str>) -> Result<Style, EditorError> {
    let underline = flag(get(row, "underline")?)?;
    let strike = flag(get(row, "strikeout")?)?;
    let encoding = uint(get(row, "encoding")?)?;
    if encoding != 0 && encoding != 1 {
        return Err(unsupported("ASS legacy font encoding is not represented"));
    }
    // Secondary/outline/shadow colours are parsed for validity, but cannot affect
    // static fill when karaoke, outline and shadow are disabled in the final cue.
    for key in ["secondarycolour", "outlinecolour", "backcolour"] {
        color(get(row, key)?)?;
    }
    Ok(Style {
        value: ParsedAssStyle {
            font_family: family(get(row, "fontname")?)?,
            font_size_px: positive(get(row, "fontsize")?)?,
            font_weight: weight(get(row, "bold")?)?,
            font_style: if flag(get(row, "italic")?)? {
                FontStyle::Italic
            } else {
                FontStyle::Normal
            },
            decoration: TextDecoration::None,
            color: color(get(row, "primarycolour")?)?,
            scale_x: positive(get(row, "scalex")?)? / 100.0,
            scale_y: positive(get(row, "scaley")?)? / 100.0,
            letter_spacing_px: number(get(row, "spacing")?)?,
            rotation_degrees: -number(get(row, "angle")?)?,
            alignment: alignment(get(row, "alignment")?)?,
        },
        margins: AssMargins {
            left: uint(get(row, "marginl")?)?,
            right: uint(get(row, "marginr")?)?,
            vertical: uint(get(row, "marginv")?)?,
        },
        underline,
        strike,
        border: u8::try_from(uint(get(row, "borderstyle")?)?)
            .map_err(|_| invalid("Invalid ASS border style"))?,
        outline: number(get(row, "outline")?)?,
        shadow: number(get(row, "shadow")?)?,
    })
}
fn override_block(
    block: &str,
    current: &mut Style,
    base: &Style,
    styles: &BTreeMap<String, Style>,
    position: &mut Option<AssPosition>,
    wrap_style: &mut u8,
    default_wrap: u8,
) -> Result<(), EditorError> {
    if block.trim().is_empty() {
        return Ok(());
    }
    if !block.starts_with('\\') {
        return Err(unsupported(
            "ASS override comments are not imported as rendering instructions",
        ));
    }
    for raw in block[1..].split('\\') {
        let names = [
            "alpha", "fscx", "fscy", "bord", "shad", "frz", "fsp", "pos", "fn", "fs", "an", "1c",
            "1a", "fr", "q", "r", "b", "i", "u", "s", "c",
        ];
        let name = names
            .iter()
            .find(|name| raw.starts_with(**name))
            .ok_or_else(|| unsupported(format!("Unsupported ASS override \\{raw}")))?;
        let value = raw[name.len()..].trim();
        if !matches!(*name, "fn" | "r") && value.starts_with(|c: char| c.is_ascii_alphabetic()) {
            return Err(unsupported(format!("Unsupported ASS override \\{raw}")));
        }
        if *name == "fs" && value.starts_with(['+', '-']) {
            return Err(unsupported(
                "Relative ASS font-size syntax is not represented",
            ));
        }

        match *name {
            "fn" => {
                current.value.font_family = if value.is_empty() {
                    base.value.font_family.clone()
                } else {
                    family(value)?
                }
            }
            "fs" => {
                current.value.font_size_px = if value.is_empty() {
                    base.value.font_size_px
                } else {
                    positive(value)?
                }
            }
            "b" => {
                current.value.font_weight = if value.is_empty() {
                    base.value.font_weight
                } else {
                    weight(value)?
                }
            }
            "i" => {
                current.value.font_style = if value.is_empty() {
                    base.value.font_style
                } else {
                    if flag(value)? {
                        FontStyle::Italic
                    } else {
                        FontStyle::Normal
                    }
                }
            }
            "u" => {
                current.underline = if value.is_empty() {
                    base.underline
                } else {
                    flag(value)?
                }
            }
            "s" => {
                current.strike = if value.is_empty() {
                    base.strike
                } else {
                    flag(value)?
                }
            }
            "1c" | "c" => {
                if !value.is_empty() {
                    hex(value, 6)?;
                }
                let c = if value.is_empty() {
                    base.value.color.clone()
                } else {
                    color(value)?
                };
                current.value.color.red = c.red;
                current.value.color.green = c.green;
                current.value.color.blue = c.blue;
            }
            "1a" | "alpha" => {
                current.value.color.alpha = if value.is_empty() {
                    base.value.color.alpha
                } else {
                    1.0 - f64::from(hex(value, 2)?) / 255.0
                }
            }
            "fscx" => {
                current.value.scale_x = if value.is_empty() {
                    base.value.scale_x
                } else {
                    positive(value)? / 100.0
                }
            }
            "fscy" => {
                current.value.scale_y = if value.is_empty() {
                    base.value.scale_y
                } else {
                    positive(value)? / 100.0
                }
            }
            "fsp" => {
                current.value.letter_spacing_px = if value.is_empty() {
                    base.value.letter_spacing_px
                } else {
                    number(value)?
                }
            }
            "frz" | "fr" => {
                current.value.rotation_degrees = if value.is_empty() {
                    base.value.rotation_degrees
                } else {
                    -number(value)?
                }
            }
            "an" => {
                current.value.alignment = if value.is_empty() {
                    base.value.alignment
                } else {
                    alignment(value)?
                }
            }
            "q" => {
                *wrap_style = if value.is_empty() {
                    default_wrap
                } else {
                    wrap(value)?
                }
            }
            "r" => {
                *current = if value.is_empty() {
                    base.clone()
                } else {
                    styles
                        .get(value)
                        .cloned()
                        .ok_or_else(|| invalid("ASS reset references missing style"))?
                };
                *wrap_style = default_wrap;
            }
            "pos" => {
                if position.is_some() {
                    return Err(unsupported("Repeated ASS position is renderer-dependent"));
                }
                let value = value
                    .strip_prefix('(')
                    .and_then(|s| s.strip_suffix(')'))
                    .ok_or_else(|| invalid("ASS pos requires (x,y)"))?;
                let (x, y) = value
                    .split_once(',')
                    .ok_or_else(|| invalid("ASS pos requires two coordinates"))?;
                *position = Some(AssPosition {
                    x: number(x)?,
                    y: number(y)?,
                });
            }
            "bord" => {
                current.outline = if value.is_empty() {
                    base.outline
                } else {
                    number(value)?
                }
            }
            "shad" => {
                current.shadow = if value.is_empty() {
                    base.shadow
                } else {
                    number(value)?
                }
            }
            _ => unreachable!(),
        }
    }
    Ok(())
}
fn cue(
    row: &BTreeMap<String, &str>,
    styles: &BTreeMap<String, Style>,
    default_wrap: u8,
) -> Result<ParsedAssCue, EditorError> {
    let start = time(get(row, "start")?)?;
    let end = time(get(row, "end")?)?;
    if end <= start {
        return Err(invalid("ASS cue end must follow start"));
    }
    let base = styles
        .get(get(row, "style")?.trim())
        .ok_or_else(|| invalid("ASS event references missing style"))?;
    let mut current = base.clone();
    let mut position = None;
    let mut wrap_style = default_wrap;
    if !get(row, "effect")?.trim().is_empty() {
        return Err(unsupported(
            "ASS event effects/scrolling are not represented",
        ));
    }
    let mut margins = base.margins;
    for (key, value) in [
        ("marginl", &mut margins.left),
        ("marginr", &mut margins.right),
        ("marginv", &mut margins.vertical),
    ] {
        let n = uint(get(row, key)?)?;
        if n != 0 {
            *value = n
        }
    }
    let mut rest = get(row, "text")?;
    let mut raw_text = String::new();
    let mut seen_text = false;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix('{') {
            let (block, next) = after
                .split_once('}')
                .ok_or_else(|| invalid("Unclosed ASS override block"))?;
            if block.contains('{') {
                return Err(invalid("Nested ASS override block"));
            }
            if seen_text && !block.trim().is_empty() {
                return Err(unsupported(
                    "ASS mid-text styling cannot be represented by uniform TextV4",
                ));
            }
            override_block(
                block,
                &mut current,
                base,
                styles,
                &mut position,
                &mut wrap_style,
                default_wrap,
            )?;
            rest = next;
        } else {
            let end = rest.find('{').unwrap_or(rest.len());
            let text = &rest[..end];
            if text.contains('}') {
                return Err(invalid("Unexpected ASS closing brace"));
            }
            raw_text.push_str(text);
            seen_text |= !text.is_empty();
            rest = &rest[end..];
        }
    }
    if current.border != 1 || current.outline != 0.0 || current.shadow != 0.0 {
        return Err(unsupported(
            "ASS outline, shadow and opaque-box borders are not represented",
        ));
    }
    if current.underline && current.strike {
        return Err(unsupported(
            "Combined ASS underline and strikeout are not represented",
        ));
    }
    current.value.decoration = if current.underline {
        TextDecoration::Underline
    } else if current.strike {
        TextDecoration::LineThrough
    } else {
        TextDecoration::None
    };
    let mut chars = raw_text.chars();
    let mut text = String::new();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('N') => text.push('\n'),
                Some('n') => text.push(if wrap_style == 2 { '\n' } else { ' ' }),
                Some('h') => text.push('\u{00a0}'),
                _ => return Err(unsupported("Unknown ASS text escape")),
            }
        } else if c.is_control() {
            return Err(invalid(
                "ASS text contains an unsupported control character",
            ));
        } else {
            text.push(c)
        }
    }
    if text.is_empty() || text.len() > 64 * 1024 {
        return Err(invalid("ASS cue text requires 1..65536 bytes"));
    }
    Ok(ParsedAssCue {
        timeline_range: TimeRange {
            start: Time {
                ticks: start.to_string(),
                timebase: 100,
            },
            duration: Time {
                ticks: (end - start).to_string(),
                timebase: 100,
            },
        },
        text,
        style: current.value,
        position,
        margins,
        wrap_style,
        layer: get(row, "layer")?
            .trim()
            .parse()
            .map_err(|_| invalid("Invalid ASS layer"))?,
    })
}

pub(crate) fn parse(content: &str) -> Result<ParsedAssDocument, EditorError> {
    if content.len() > 2 * 1024 * 1024 || content.contains('\0') {
        return Err(invalid("ASS input exceeds 2 MiB or contains NUL"));
    }
    let mut section = String::new();
    let mut width = None;
    let mut height = None;
    let mut wrap_style = 0;
    let mut info_keys = BTreeSet::new();
    let mut style_format = None;
    let mut event_format = None;
    let mut styles = BTreeMap::new();
    let mut events = Vec::new();
    for raw in content.trim_start_matches('\u{feff}').lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line.to_ascii_lowercase();
            match section.as_str() {
                "[script info]" | "[v4+ styles]" | "[events]" => {}
                "[aegisub project garbage]" => {}
                _ => {
                    return Err(unsupported(format!(
                        "Unsupported ASS section {line}; attachments and legacy styles are not imported"
                    )));
                }
            }
            continue;
        }
        // Dialogue Text is the last field; its trailing whitespace is content.
        // Trim only structural lines above, not the payload being parsed here.
        let (key, value) = raw
            .split_once(':')
            .ok_or_else(|| invalid("ASS line lacks field separator"))?;
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim_start();
        match section.as_str() {
            "[script info]" => {
                if !info_keys.insert(key.clone()) {
                    return Err(invalid("Duplicate ASS Script Info field"));
                }
                match key.as_str() {
                    "playresx" => width = Some(uint(value)?),
                    "playresy" => height = Some(uint(value)?),
                    "wrapstyle" => wrap_style = wrap(value)?,
                    "scripttype" => {
                        if value.trim() != "v4.00+" {
                            return Err(unsupported("Only ASS v4.00+ scripts are represented"));
                        }
                    }
                    "timer" => {
                        if number(value)? != 100.0 {
                            return Err(unsupported("ASS Timer changes are not represented"));
                        }
                    }
                    "scaledborderandshadow" => {
                        if !matches!(value.trim().to_ascii_lowercase().as_str(), "yes" | "no") {
                            return Err(invalid("Invalid ScaledBorderAndShadow"));
                        }
                    }
                    "ycbcr matrix" => {
                        if value.trim() != "None" {
                            return Err(unsupported(
                                "ASS YCbCr matrix conversion is not represented",
                            ));
                        }
                    }
                    "title"
                    | "original script"
                    | "original translation"
                    | "original editing"
                    | "original timing"
                    | "synch point"
                    | "script updated by"
                    | "update details" => {}
                    _ => {
                        return Err(unsupported(format!(
                            "Unsupported ASS Script Info field {key}"
                        )));
                    }
                }
            }
            "[v4+ styles]" => match key.as_str() {
                "format" => {
                    if style_format.is_some() {
                        return Err(invalid("Repeated ASS style Format"));
                    }
                    let fields = format(value)?;
                    check_fields(
                        &fields,
                        &[
                            "name",
                            "fontname",
                            "fontsize",
                            "primarycolour",
                            "secondarycolour",
                            "outlinecolour",
                            "backcolour",
                            "bold",
                            "italic",
                            "underline",
                            "strikeout",
                            "scalex",
                            "scaley",
                            "spacing",
                            "angle",
                            "borderstyle",
                            "outline",
                            "shadow",
                            "alignment",
                            "marginl",
                            "marginr",
                            "marginv",
                            "encoding",
                        ],
                    )?;
                    style_format = Some(fields)
                }
                "style" => {
                    if styles.len() >= 5000 {
                        return Err(invalid("ASS style limit is 5000"));
                    }
                    let row = row(
                        style_format
                            .as_ref()
                            .ok_or_else(|| invalid("ASS style lacks Format"))?,
                        value,
                        false,
                    )?;
                    let name = get(&row, "name")?.trim().to_owned();
                    if name.is_empty() || styles.insert(name, style(&row)?).is_some() {
                        return Err(invalid("Empty or duplicate ASS style name"));
                    }
                }
                _ => return Err(unsupported("Unknown ASS style row")),
            },
            "[events]" => match key.as_str() {
                "format" => {
                    if event_format.is_some() {
                        return Err(invalid("Repeated ASS event Format"));
                    }
                    let fields = format(value)?;
                    check_fields(
                        &fields,
                        &[
                            "layer", "start", "end", "style", "name", "marginl", "marginr",
                            "marginv", "effect", "text",
                        ],
                    )?;
                    event_format = Some(fields)
                }
                "dialogue" => {
                    if events.len() >= 5000 {
                        return Err(invalid("ASS cue limit is 5000"));
                    }
                    events.push(
                        row(
                            event_format
                                .as_ref()
                                .ok_or_else(|| invalid("ASS event lacks Format"))?,
                            value,
                            true,
                        )?
                        .into_iter()
                        .map(|(k, v)| (k, v.to_owned()))
                        .collect::<BTreeMap<_, _>>(),
                    )
                }
                "comment" => {}
                _ => return Err(unsupported("Unknown ASS event row")),
            },
            "[aegisub project garbage]" => {}
            _ => return Err(invalid("ASS data outside represented sections")),
        }
    }
    let width = width.ok_or_else(|| invalid("ASS PlayResX is required"))?;
    let height = height.ok_or_else(|| invalid("ASS PlayResY is required"))?;
    if width == 0 || height == 0 || width > 32768 || height > 32768 {
        return Err(invalid("ASS script dimensions exceed 1..32768"));
    }
    let cues = events
        .iter()
        .map(|row| {
            cue(
                &row.iter().map(|(k, v)| (k.clone(), v.as_str())).collect(),
                &styles,
                wrap_style,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    if cues.is_empty() {
        return Err(invalid("ASS requires at least one Dialogue cue"));
    }
    Ok(ParsedAssDocument {
        width,
        height,
        wrap_style,
        cues,
    })
}
#[cfg(test)]
#[path = "parse_tests.rs"]
mod tests;
