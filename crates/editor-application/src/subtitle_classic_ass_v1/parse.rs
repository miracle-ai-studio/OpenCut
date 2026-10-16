//! Direct port of Classic subtitles/ass.ts. This is the compatibility policy,
//! deliberately distinct from the frozen strict external-ASS parser.
use super::*;
use std::collections::BTreeMap;
#[derive(Clone, Debug)]
pub(super) struct ParsedCue {
    pub range: TimeRange,
    pub text: String,
    pub style: ClassicStyle,
}
#[derive(Clone, Debug)]
pub(super) struct ClassicStyle {
    pub family: String,
    pub size: f64,
    pub weight: FontWeightV4,
    pub style: FontStyleV4,
    pub decoration: TextDecorationV4,
    pub color: ColorV4,
    pub align: TextAlignV4,
    pub spacing: f64,
    pub background: TextBackgroundV4,
    pub layout: SubtitleCueLayoutV1,
}
impl Default for ClassicStyle {
    fn default() -> Self {
        Self {
            family: "Arial".into(),
            size: 5.,
            weight: FontWeightV4::Bold,
            style: FontStyleV4::Normal,
            decoration: TextDecorationV4::None,
            color: rgba(1., 1., 1., 1.),
            align: TextAlignV4::Center,
            spacing: 0.,
            background: TextBackgroundV4 {
                enabled: false,
                color: rgba(0., 0., 0., 1.),
                corner_radius_percent: 0.,
                padding_x_at_size15: 30.,
                padding_y_at_size15: 42.,
                offset_x_px: 0.,
                offset_y_px: 0.,
            },
            layout: SubtitleCueLayoutV1 {
                max_width_px: None,
                left_margin_ratio: 0.,
                right_margin_ratio: 0.,
                vertical_margin_ratio: 0.05,
                vertical_align: 0,
                allow_zero_width: true,
            },
        }
    }
}
pub(super) struct Parsed {
    pub cues: Vec<ParsedCue>,
    pub warnings: Vec<String>,
    pub skipped: u32,
}
fn rgba(red: f64, green: f64, blue: f64, alpha: f64) -> ColorV4 {
    ColorV4 {
        encoding: ColorEncodingV4::Srgb,
        red,
        green,
        blue,
        alpha,
    }
}
// ECMAScript whitespace includes BOM, while Rust's Unicode trim does not.
fn space(c: char) -> bool {
    matches!(c,'\u{0009}'..='\u{000d}'|'\u{0020}'|'\u{00a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}')
}
fn trim(s: &str) -> &str {
    s.trim_matches(space)
}
fn float(s: &str) -> Option<f64> {
    let s = s.trim_start_matches(space);
    let mut end = 0;
    for (i, c) in s.char_indices() {
        if c.is_ascii_digit() || matches!(c, '+' | '-' | '.' | 'e' | 'E') {
            end = i + c.len_utf8();
        } else {
            break;
        }
    }
    while end > 0 {
        if let Ok(n) = s[..end].parse::<f64>() {
            return n.is_finite().then_some(n);
        }
        end -= 1;
    }
    None
}
fn integer(s: &str) -> Option<f64> {
    let s = trim(s);
    let start = usize::from(s.starts_with(['+', '-']));
    let mut end = start;
    for (i, c) in s[start..].char_indices() {
        if !c.is_ascii_digit() {
            break;
        }
        end = start + i + 1;
    }
    if end <= start {
        return None;
    }
    s[..end].parse::<f64>().ok().filter(|n| n.is_finite())
}
fn flag(s: Option<&&str>) -> Option<bool> {
    s.and_then(|s| integer(s)).map(|n| n != 0.)
}
fn color(s: &str) -> Option<ColorV4> {
    let s = trim(s);
    let s = if s
        .as_bytes()
        .get(..2)
        .is_some_and(|p| p.eq_ignore_ascii_case(b"&h"))
    {
        &s[2..]
    } else if s.starts_with(['h', 'H']) {
        &s[1..]
    } else {
        s
    };
    if s.is_empty() || s.len() > 8 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let n = u32::from_str_radix(s, 16).ok()?;
    // Classic serializes alpha to CSS with 3 decimal places before drawing.
    let alpha = 1. - f64::from(n >> 24) / 255.;
    Some(rgba(
        f64::from(n & 255) / 255.,
        f64::from((n >> 8) & 255) / 255.,
        f64::from((n >> 16) & 255) / 255.,
        if alpha >= 1. {
            1.
        } else {
            (alpha * 1000. + 0.5).floor() / 1000.
        },
    ))
}
fn style(row: &BTreeMap<String, &str>, width: f64, height: f64) -> (ClassicStyle, bool) {
    let mut out = ClassicStyle::default();
    let get = |k| row.get(k).and_then(|v| float(v));
    if let Some(family) = row.get("fontname").filter(|s| !s.is_empty()) {
        out.family = trim(family).into()
    }
    if let Some(size) = get("fontsize") {
        let ratio = (size / height * 1000. + 0.5).floor() / 1000.;
        if ratio > 0. {
            out.size = ratio * 90.;
        }
    }
    if let Some(c) = row.get("primarycolour").and_then(|s| color(s)) {
        out.color = c
    }
    if let Some(b) = flag(row.get("bold")) {
        out.weight = if b {
            FontWeightV4::Bold
        } else {
            FontWeightV4::Normal
        }
    }
    if let Some(b) = flag(row.get("italic")) {
        out.style = if b {
            FontStyleV4::Italic
        } else {
            FontStyleV4::Normal
        }
    }
    let underline = flag(row.get("underline")).unwrap_or(false);
    let strike = flag(row.get("strikeout")).unwrap_or(false);
    out.decoration = if underline {
        TextDecorationV4::Underline
    } else if strike {
        TextDecorationV4::LineThrough
    } else {
        TextDecorationV4::None
    };
    out.spacing = get("spacing").unwrap_or(0.);
    let a = get("alignment")
        .map(|n| (n + 0.5).floor() as i64)
        .filter(|n| (1..=9).contains(n))
        .unwrap_or(2);
    out.layout.vertical_align = ((a - 1) / 3) as u8;
    out.align = match (a - 1) % 3 {
        0 => TextAlignV4::Left,
        2 => TextAlignV4::Right,
        _ => TextAlignV4::Center,
    };
    out.layout.left_margin_ratio = get("marginl").map(|n| n / width).unwrap_or(0.);
    out.layout.right_margin_ratio = get("marginr").map(|n| n / width).unwrap_or(0.);
    out.layout.vertical_margin_ratio = get("marginv").map(|n| n / height).unwrap_or(0.05);
    let border = get("borderstyle").map(|n| (n + 0.5).floor() as i64);
    if border == Some(3) {
        if let Some(c) = row.get("backcolour").and_then(|s| color(s)) {
            out.background.enabled = c.alpha > 0.;
            out.background.color = if c.alpha > 0. {
                c
            } else {
                rgba(0., 0., 0., 0.)
            };
        }
    }
    let unsupported = !matches!(border, Some(1 | 3))
        || get("outline").unwrap_or(0.) > 0.
        || get("shadow").unwrap_or(0.) > 0.
        || get("angle").unwrap_or(0.) != 0.
        || get("scalex").is_some_and(|n| n != 100.)
        || get("scaley").is_some_and(|n| n != 100.)
        || (underline && strike);
    (out, unsupported)
}
fn fields<'a>(format: &[String], values: &'a str) -> Option<BTreeMap<String, &'a str>> {
    // Classic only recognizes delimiters before the last declared field, so
    // commas in Dialogue Text remain content.
    let values = values
        .splitn(format.len(), ',')
        .map(trim)
        .collect::<Vec<_>>();
    (values.len() == format.len()).then(|| format.iter().cloned().zip(values).collect())
}
fn clock(s: &str) -> Option<f64> {
    let p = trim(s).split(':').collect::<Vec<_>>();
    if p.len() != 3 || p[0].is_empty() || p[1].len() != 2 {
        return None;
    }
    let (sec, frac) = p[2].split_once('.')?;
    if sec.len() != 2
        || !(1..=3).contains(&frac.len())
        || [p[0], p[1], sec, frac]
            .iter()
            .any(|s| !s.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    let h = p[0].parse::<u64>().ok()? as f64;
    let m = p[1].parse::<u64>().ok()? as f64;
    let s = sec.parse::<u64>().ok()? as f64;
    let f = frac.parse::<u64>().ok()? as f64 / 10f64.powi(frac.len() as i32);
    Some(h * 3600. + m * 60. + s + f)
}
fn ticks(seconds: f64) -> Result<Time, EditorError> {
    let n = (seconds * 120000. + 0.5).floor();
    if !n.is_finite() || n < 0. || n > i64::MAX as f64 {
        return Err(invalid("Classic ASS timestamp exceeds the supported clock"));
    }
    Ok(Time {
        ticks: (n as i64).to_string(),
        timebase: 120000,
    })
}
fn strip(s: &str) -> (String, bool) {
    let mut raw = String::new();
    let mut rest = s;
    let mut tags = false;
    while let Some(i) = rest.find('{') {
        raw.push_str(&rest[..i]);
        if let Some(end) = rest[i + 1..].find('}') {
            tags = true;
            rest = &rest[i + end + 2..];
        } else {
            raw.push_str(&rest[i..]);
            rest = "";
            break;
        }
    }
    raw.push_str(rest);
    let mut result = String::new();
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.peek() {
                Some('N' | 'n') => {
                    result.push('\n');
                    chars.next();
                    continue;
                }
                Some('h') => {
                    result.push(' ');
                    chars.next();
                    continue;
                }
                _ => {}
            }
        }
        result.push(c)
    }
    (trim(&result).into(), tags)
}
pub(super) fn parse(content: &str) -> Result<Parsed, EditorError> {
    if content.len() > 2 * 1024 * 1024 || content.contains('\0') {
        return Err(invalid("Classic ASS exceeds 2 MiB or contains NUL"));
    }
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
    let mut section = String::new();
    let (mut width, mut height) = (384., 288.);
    let mut style_format = None;
    let mut event_format = None;
    let mut styles = BTreeMap::new();
    let mut output = Parsed {
        cues: vec![],
        warnings: vec![],
        skipped: 0,
    };
    let (mut tags, mut effects, mut missing, mut nondialogue, mut unsupported) =
        (0, 0, 0, 0, false);
    for raw in trim(&normalized).split('\n') {
        let line = trim(raw);
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = trim(&line[1..line.len() - 1]).to_lowercase();
            continue;
        }
        if section == "script info" {
            if let Some((key, value)) = line.split_once(':') {
                if let Some(n) = float(value).filter(|n| *n > 0.) {
                    match trim(key).to_lowercase().as_str() {
                        "playresx" => width = n,
                        "playresy" => height = n,
                        _ => {}
                    }
                }
            }
            continue;
        }
        if matches!(section.as_str(), "v4 styles" | "v4+ styles") {
            if let Some(value) = line.strip_prefix("Format:") {
                style_format = Some(
                    value
                        .split(',')
                        .map(|s| trim(s).to_lowercase())
                        .collect::<Vec<_>>(),
                );
                continue;
            }
            if let Some(value) = line.strip_prefix("Style:") {
                if let Some(format) = &style_format {
                    if let Some(row) = fields(format, trim(value)) {
                        if let Some(name) = row.get("name").filter(|s| !s.is_empty()) {
                            let (value, loss) = style(&row, width, height);
                            unsupported |= loss;
                            styles.insert(name.to_lowercase(), value);
                        }
                    }
                }
            }
            continue;
        }
        if section != "events" {
            continue;
        }
        if let Some(value) = line.strip_prefix("Format:") {
            event_format = Some(
                value
                    .split(',')
                    .map(|s| trim(s).to_lowercase())
                    .collect::<Vec<_>>(),
            );
            continue;
        }
        let Some(format) = &event_format else {
            continue;
        };
        if let Some(value) = line.strip_prefix("Dialogue:") {
            let Some(row) = fields(format, trim(value)) else {
                output.skipped += 1;
                continue;
            };
            let Some(start) = row.get("start").and_then(|s| clock(s)) else {
                output.skipped += 1;
                continue;
            };
            let Some(end) = row
                .get("end")
                .and_then(|s| clock(s))
                .filter(|end| *end > start)
            else {
                output.skipped += 1;
                continue;
            };
            let (text, had) = strip(row.get("text").copied().unwrap_or(""));
            if had {
                tags += 1
            }
            if text.is_empty() {
                output.skipped += 1;
                continue;
            }
            if row.get("effect").is_some_and(|s| !trim(s).is_empty()) {
                effects += 1
            }
            let name = row
                .get("style")
                .map(|s| trim(s).to_lowercase())
                .filter(|s| !s.is_empty());
            let selected = name.as_ref().and_then(|n| styles.get(n));
            if name.is_some() && selected.is_none() {
                missing += 1
            }
            let style = selected
                .or_else(|| styles.get("default"))
                .cloned()
                .unwrap_or_default();
            output.cues.push(ParsedCue {
                range: TimeRange {
                    start: ticks(start)?,
                    duration: ticks(end - start)?,
                },
                text,
                style,
            });
            if output.cues.len() > 5000 {
                return Err(invalid("Classic ASS cue limit is 5000"));
            }
        } else if line.contains(':') {
            nondialogue += 1
        }
    }
    if tags > 0 {
        output.warnings.push(format!(
            "Stripped unsupported ASS inline override tags from {tags} subtitle cue(s)."
        ));
    }
    if effects > 0 {
        output.warnings.push(format!(
            "Ignored ASS event effects in {effects} subtitle cue(s)."
        ));
    }
    if missing > 0 {
        output.warnings.push(format!("Fell back to default subtitle styling for {missing} cue(s) that referenced missing ASS styles."));
    }
    if nondialogue > 0 {
        output
            .warnings
            .push(format!("Ignored {nondialogue} non-dialogue ASS event(s)."));
    }
    if unsupported {
        output.warnings.push(
            "Ignored unsupported ASS style features such as outline, shadow, rotation, or scaling."
                .into(),
        );
    }
    Ok(output)
}
