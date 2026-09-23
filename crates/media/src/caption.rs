use crate::{
    FontResolutionPolicy, MediaError, MediaResult, PixelBounds, TextAlign, TextClusterRange,
    TextLayoutReport, TimelineCaption, UsedFontFace,
};
use cosmic_text::{
    Align, Attrs, Buffer, Color, Fallback, Family, FontSystem, Metrics, Shaping, SwashCache, Wrap,
    fontdb,
};
use image::{Rgba, RgbaImage};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    path::Path,
    sync::{Mutex, OnceLock},
};
use unicode_script::Script;

#[derive(Debug)]
struct OrderedFallback {
    families: Vec<&'static str>,
}

impl OrderedFallback {
    fn new(families: &[String]) -> MediaResult<Self> {
        const MAX_INTERNED_FAMILIES: usize = 256;
        static INTERNED: OnceLock<Mutex<BTreeMap<String, &'static str>>> = OnceLock::new();
        let mut interned = INTERNED
            .get_or_init(|| Mutex::new(BTreeMap::new()))
            .lock()
            .map_err(|_| MediaError::InvalidToolOutput {
                tool: "cosmic-text".into(),
                detail: "font family interner was poisoned".into(),
            })?;
        let mut ordered = Vec::with_capacity(families.len());
        for family in families {
            let value = if let Some(value) = interned.get(family) {
                *value
            } else {
                if interned.len() >= MAX_INTERNED_FAMILIES {
                    return Err(MediaError::Unsupported(format!(
                        "at most {MAX_INTERNED_FAMILIES} distinct fallback font families may be registered per process"
                    )));
                }
                let value = Box::leak(family.clone().into_boxed_str()) as &'static str;
                interned.insert(family.clone(), value);
                value
            };
            ordered.push(value);
        }
        Ok(Self { families: ordered })
    }
}

impl Fallback for OrderedFallback {
    fn common_fallback(&self) -> &[&'static str] {
        &self.families
    }

    fn forbidden_fallback(&self) -> &[&'static str] {
        &[]
    }

    fn script_fallback(&self, _script: Script, _locale: &str) -> &[&'static str] {
        &[]
    }
}

#[cfg(test)]
pub(crate) fn render(
    c: &TimelineCaption,
    width: u32,
    height: u32,
    output: &Path,
) -> MediaResult<TextLayoutReport> {
    render_admitted(c, width, height, output, None)
}

pub(crate) fn render_admitted(
    c: &TimelineCaption,
    width: u32,
    height: u32,
    output: &Path,
    admission: Option<&dyn crate::OutputAdmission>,
) -> MediaResult<TextLayoutReport> {
    render_impl(c, width, height, Some(output), admission)
}

pub(crate) fn layout(
    c: &TimelineCaption,
    width: u32,
    height: u32,
) -> MediaResult<TextLayoutReport> {
    render_impl(c, width, height, None, None)
}

fn render_impl(
    c: &TimelineCaption,
    width: u32,
    height: u32,
    output: Option<&Path>,
    admission: Option<&dyn crate::OutputAdmission>,
) -> MediaResult<TextLayoutReport> {
    if c.text.is_empty() {
        return Err(MediaError::InvalidRequest(
            "caption text must not be empty".into(),
        ));
    }
    let families = match &c.font_policy {
        FontResolutionPolicy::Strict {} => vec![c.font_family.clone()],
        FontResolutionPolicy::ExplicitFallback { families } => {
            let mut v = vec![c.font_family.clone()];
            v.extend(families.clone());
            v
        }
    };
    if families.iter().any(|v| v.trim().is_empty()) {
        return Err(MediaError::InvalidRequest(
            "font family names must not be empty".into(),
        ));
    }
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    for family in &families {
        if !db.faces().any(|f| {
            f.families
                .iter()
                .any(|(n, _)| n.eq_ignore_ascii_case(family))
        }) {
            return Err(MediaError::Unsupported(format!(
                "font family {family} is unavailable"
            )));
        }
    }
    let allowed: HashSet<_> = db
        .faces()
        .filter(|f| {
            f.families
                .iter()
                .any(|(n, _)| families.iter().any(|w| n.eq_ignore_ascii_case(w)))
        })
        .map(|f| f.id)
        .collect();
    let remove: Vec<_> = db
        .faces()
        .filter(|f| !allowed.contains(&f.id))
        .map(|f| f.id)
        .collect();
    for id in remove {
        db.remove_face(id);
    }
    let ordered_fallback = match &c.font_policy {
        FontResolutionPolicy::Strict {} => Vec::new(),
        FontResolutionPolicy::ExplicitFallback { families } => families.clone(),
    };
    let mut fonts = FontSystem::new_with_locale_and_db_and_fallback(
        "en-US".into(),
        db,
        OrderedFallback::new(&ordered_fallback)?,
    );
    let size = c.font_size as f32;
    if !size.is_finite() || size <= 0.0 {
        return Err(MediaError::InvalidRequest(
            "caption font size must be positive".into(),
        ));
    }
    let line_height = c.line_height.unwrap_or(size * 1.25);
    if !line_height.is_finite() || line_height < size {
        return Err(MediaError::InvalidRequest(
            "caption line height must be finite and at least font size".into(),
        ));
    }
    let max_width = c
        .max_width
        .unwrap_or(width.saturating_sub(32))
        .min(width)
        .max(1);
    let mut buffer = Buffer::new(&mut fonts, Metrics::new(size, line_height));
    buffer.set_size(Some(max_width as f32), Some(height as f32));
    buffer.set_wrap(Wrap::WordOrGlyph);
    let align = match c.text_align {
        TextAlign::Left => Align::Left,
        TextAlign::Center => Align::Center,
        TextAlign::Right => Align::Right,
    };
    buffer.set_text(
        &c.text,
        &Attrs::new().family(Family::Name(&c.font_family)),
        Shaping::Advanced,
        Some(align),
    );
    buffer.shape_until_scroll(&mut fonts, false);
    let mut ids = BTreeSet::new();
    let mut clusters = Vec::new();
    let mut lines = 0;
    let mut glyphs = 0;
    let mut rtl = 0;
    let mut line_offsets = Vec::new();
    let mut offset = 0;
    for line in c.text.split('\n') {
        line_offsets.push(offset);
        offset += line.len() + 1;
    }
    for run in buffer.layout_runs() {
        lines += 1;
        if run.rtl {
            rtl += 1;
        }
        for g in run.glyphs {
            if g.glyph_id == 0 {
                return Err(MediaError::Unsupported(format!(
                    "configured fonts do not contain caption cluster bytes {}..{}",
                    g.start, g.end
                )));
            }
            ids.insert(g.font_id);
            glyphs += 1;
            let line_offset = line_offsets.get(run.line_i).copied().unwrap_or(0);
            clusters.push(TextClusterRange {
                start: line_offset + g.start,
                end: line_offset + g.end,
                rtl: g.level.is_rtl(),
            });
        }
    }
    if glyphs == 0 {
        return Err(MediaError::Unsupported(
            "caption shaped to no visible glyphs".into(),
        ));
    }
    let db = fonts.db();
    let faces = ids
        .iter()
        .map(|id| {
            let f = db.face(*id).ok_or_else(|| MediaError::InvalidToolOutput {
                tool: "cosmic-text".into(),
                detail: "used font disappeared".into(),
            })?;
            Ok(UsedFontFace {
                family: f.families.first().map(|v| v.0.clone()).unwrap_or_default(),
                postscript_name: f.post_script_name.clone(),
                source: compact_font_source(&f.source),
                face_index: f.index,
            })
        })
        .collect::<MediaResult<Vec<_>>>()?;
    if matches!(c.font_policy, FontResolutionPolicy::Strict {})
        && faces
            .iter()
            .any(|f| !f.family.eq_ignore_ascii_case(&c.font_family))
    {
        return Err(MediaError::Unsupported(format!(
            "font {} lacks glyphs and strict policy forbids fallback",
            c.font_family
        )));
    }
    let rgba = parse_color(&c.color)?;
    let mut cache = SwashCache::new();
    let mut pixels = Vec::new();
    buffer.draw(
        &mut fonts,
        &mut cache,
        Color::rgba(rgba.0[0], rgba.0[1], rgba.0[2], rgba.0[3]),
        |x, y, w, h, color| pixels.push((x, y, w, h, color)),
    );
    if pixels.is_empty() {
        return Err(MediaError::Unsupported(
            "caption rasterized to no visible pixels".into(),
        ));
    }
    let min_x = pixels.iter().map(|p| p.0).min().unwrap();
    let min_y = pixels.iter().map(|p| p.1).min().unwrap();
    let max_x = pixels.iter().map(|p| p.0 + p.2 as i32).max().unwrap();
    let max_y = pixels.iter().map(|p| p.1 + p.3 as i32).max().unwrap();
    let bw = (max_x - min_x).max(1) as u32;
    let bh = (max_y - min_y).max(1) as u32;
    if bw > width || bh + 40 > height {
        return Err(MediaError::InvalidRequest(format!(
            "caption layout {bw}x{bh} exceeds {width}x{height} canvas"
        )));
    }
    // Center the configured layout box, while preserving Cosmic Text's glyph offsets inside it.
    // Re-centering the ink bounds here would erase Left/Center/Right alignment.
    let ox = ((width - max_width) / 2) as i32;
    let oy = (height - bh - 40) as i32 - min_y;
    let final_min_x = min_x + ox;
    let final_max_x = max_x + ox;
    if final_min_x < 0 || final_max_x > width as i32 {
        return Err(MediaError::InvalidRequest(format!(
            "caption ink extends outside the {width}px canvas"
        )));
    }
    let bounds = PixelBounds {
        x: final_min_x as u32,
        y: (min_y + oy) as u32,
        width: bw,
        height: bh,
    };
    let mut image = RgbaImage::new(width, height);
    if let Some(bg) = &c.background_color {
        let color = parse_color(bg)?;
        for y in bounds.y.saturating_sub(4)..(bounds.y + bounds.height + 4).min(height) {
            for x in bounds.x.saturating_sub(8)..(bounds.x + bounds.width + 8).min(width) {
                image.put_pixel(x, y, color);
            }
        }
    }
    for (x, y, w, h, color) in pixels {
        for py in 0..h {
            for px in 0..w {
                let dx = x + ox + px as i32;
                let dy = y + oy + py as i32;
                if dx >= 0 && dy >= 0 && (dx as u32) < width && (dy as u32) < height {
                    blend(
                        image.get_pixel_mut(dx as u32, dy as u32),
                        Rgba([color.r(), color.g(), color.b(), color.a()]),
                    );
                }
            }
        }
    }
    if let Some(output) = output {
        use image::ImageEncoder;
        let mut encoded = Vec::new();
        image::codecs::png::PngEncoder::new(&mut encoded)
            .write_image(
                image.as_raw(),
                width,
                height,
                image::ExtendedColorType::Rgba8,
            )
            .map_err(|error| MediaError::Io(std::io::Error::other(error)))?;
        if let Some(admission) = admission {
            admission.write(output, &encoded)?;
        } else {
            std::fs::write(output, encoded)?;
        }
    }
    Ok(TextLayoutReport {
        requested_family: c.font_family.clone(),
        policy: c.font_policy.clone(),
        used_faces: faces,
        line_count: lines,
        glyph_count: glyphs,
        rtl_runs: rtl,
        cluster_ranges: clusters,
        pixel_bounds: bounds,
    })
}

fn compact_font_source(source: &fontdb::Source) -> String {
    match source {
        fontdb::Source::File(path) | fontdb::Source::SharedFile(path, _) => {
            format!("file:{}", path.to_string_lossy())
        }
        fontdb::Source::Binary(data) => {
            format!("binary:opaque:{}bytes", data.as_ref().as_ref().len())
        }
    }
}
fn blend(d: &mut Rgba<u8>, s: Rgba<u8>) {
    let a = s.0[3] as f32 / 255.0;
    for i in 0..3 {
        d.0[i] = (s.0[i] as f32 * a + d.0[i] as f32 * (1.0 - a)).round() as u8;
    }
    d.0[3] = ((a + d.0[3] as f32 / 255.0 * (1.0 - a)) * 255.0).round() as u8;
}
fn parse_color(v: &str) -> MediaResult<Rgba<u8>> {
    let (b, a) = v
        .split_once('@')
        .map_or((v, 1.0), |(x, y)| (x, y.parse::<f32>().unwrap_or(-1.0)));
    let r = match b.to_ascii_lowercase().as_str() {
        "white" => (255, 255, 255),
        "black" => (0, 0, 0),
        "red" => (255, 0, 0),
        "green" => (0, 128, 0),
        "blue" => (0, 0, 255),
        x if x.is_ascii()
            && x.starts_with('#')
            && x.len() == 7
            && x.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit) =>
        {
            (
                u8::from_str_radix(&x[1..3], 16)
                    .map_err(|_| MediaError::InvalidRequest("invalid color".into()))?,
                u8::from_str_radix(&x[3..5], 16)
                    .map_err(|_| MediaError::InvalidRequest("invalid color".into()))?,
                u8::from_str_radix(&x[5..7], 16)
                    .map_err(|_| MediaError::InvalidRequest("invalid color".into()))?,
            )
        }
        _ => {
            return Err(MediaError::Unsupported(format!(
                "caption color {v} is unsupported"
            )));
        }
    };
    if !(0.0..=1.0).contains(&a) {
        return Err(MediaError::InvalidRequest(
            "caption alpha must be 0..1".into(),
        ));
    }
    Ok(Rgba([r.0, r.1, r.2, (a * 255.0).round() as u8]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn caption(text: &str, family: &str) -> TimelineCaption {
        TimelineCaption {
            start: crate::MediaTime {
                ticks: 0,
                timebase: 1,
            },
            duration: crate::MediaTime {
                ticks: 1,
                timebase: 1,
            },
            text: text.into(),
            font_family: family.into(),
            font_size: 44.0,
            color: "white".into(),
            background_color: Some("black@0.5".into()),
            font_policy: FontResolutionPolicy::Strict {},
            max_width: Some(360),
            line_height: Some(56.0),
            text_align: TextAlign::Center,
        }
    }

    fn render_test(caption: &TimelineCaption) -> MediaResult<(TextLayoutReport, RgbaImage)> {
        let dir = tempdir().unwrap();
        let output = dir.path().join("caption.png");
        let report = render(caption, 400, 260, &output)?;
        let image = image::open(output).unwrap().into_rgba8();
        Ok((report, image))
    }

    #[test]
    fn shapes_mixed_latin_cjk_with_ordered_explicit_fallback() {
        let mut c = caption("OpenCut 中文", "Arial");
        c.font_policy = FontResolutionPolicy::ExplicitFallback {
            families: vec!["Arial Unicode MS".into(), "Hiragino Sans GB".into()],
        };
        let (report, image) = render_test(&c).unwrap();
        assert!(report.used_faces.iter().any(|face| face.family == "Arial"));
        assert!(
            report
                .used_faces
                .iter()
                .any(|face| face.family == "Arial Unicode MS")
        );
        assert!(
            !report
                .used_faces
                .iter()
                .any(|face| face.family == "Hiragino Sans GB")
        );
        let json = serde_json::to_vec(&report).unwrap();
        assert!(
            json.len() < 32 * 1024,
            "layout report is {} bytes",
            json.len()
        );
        assert!(
            report
                .used_faces
                .iter()
                .all(|face| face.source.len() < 4096)
        );
        assert!(report.used_faces.iter().all(|face| {
            face.source.starts_with("file:") || face.source.starts_with("binary:opaque:")
        }));
        assert!(image.pixels().any(|pixel| pixel.0[3] != 0));
        assert!(report.pixel_bounds.x + report.pixel_bounds.width <= image.width());
        assert!(report.pixel_bounds.y + report.pixel_bounds.height <= image.height());
    }

    #[test]
    fn shapes_rtl_and_combining_clusters() {
        let (arabic, _) = render_test(&caption("السلام عليكم", "Geeza Pro")).unwrap();
        assert!(arabic.rtl_runs > 0);
        assert!(arabic.cluster_ranges.iter().any(|cluster| cluster.rtl));
        assert!(arabic.glyph_count < "السلام عليكم".chars().count() as u32);

        let (combining, _) = render_test(&caption("Cafe\u{301}", "Arial")).unwrap();
        assert!(
            combining
                .cluster_ranges
                .iter()
                .any(|cluster| cluster.end.saturating_sub(cluster.start) > 1)
        );
    }

    #[test]
    fn wraps_multiline_and_uses_nonzero_ttc_face() {
        let mut c = caption(
            "第一行中文和 Latin words\n第二行会继续自动换行 testing wrapping",
            ".Hiragino Sans GB Interface",
        );
        c.font_size = 24.0;
        c.line_height = Some(30.0);
        c.max_width = Some(180);
        c.text_align = TextAlign::Left;
        let (report, image) = render_test(&c).unwrap();
        assert!(report.line_count >= 3);
        assert!(report.used_faces.iter().all(|face| face.face_index > 0));
        let second_line = c.text.find('\n').unwrap() + 1;
        assert!(
            report
                .cluster_ranges
                .iter()
                .any(|cluster| cluster.start >= second_line)
        );
        assert!(
            report
                .cluster_ranges
                .iter()
                .all(|cluster| cluster.end <= c.text.len())
        );
        assert!(report.pixel_bounds.width <= 180);
        assert!(report.pixel_bounds.y + report.pixel_bounds.height <= image.height());
    }

    #[test]
    fn rejects_missing_family_and_missing_glyph() {
        assert!(
            serde_json::from_str::<FontResolutionPolicy>(
                r#"{"mode":"strict","families":["Arial"]}"#
            )
            .is_err()
        );
        let missing_family = render_test(&caption("text", "OpenCut Missing Font 918273"))
            .unwrap_err()
            .to_string();
        assert!(missing_family.contains("unavailable"));

        let missing_glyph = render_test(&caption("\u{10ffff}", "Arial"))
            .unwrap_err()
            .to_string();
        assert!(missing_glyph.contains("do not contain caption cluster"));

        let mut bad_color = caption("text", "Arial");
        bad_color.color = "#aéabc".into();
        assert!(
            render_test(&bad_color)
                .unwrap_err()
                .to_string()
                .contains("unsupported")
        );
    }

    #[test]
    fn preserves_single_and_multiline_alignment_inside_centered_layout_box() {
        fn x_for(text: &str, align: TextAlign) -> u32 {
            let mut c = caption(text, "Arial");
            c.max_width = Some(300);
            c.text_align = align;
            render_test(&c).unwrap().0.pixel_bounds.x
        }

        let left = x_for("alignment", TextAlign::Left);
        let center = x_for("alignment", TextAlign::Center);
        let right = x_for("alignment", TextAlign::Right);
        assert!(left < center && center < right, "{left} {center} {right}");

        let left = x_for("short\na much longer second line", TextAlign::Left);
        let center = x_for("short\na much longer second line", TextAlign::Center);
        let right = x_for("short\na much longer second line", TextAlign::Right);
        assert!(left < center && center < right, "{left} {center} {right}");
    }

    #[test]
    fn writes_persistent_unicode_caption_qa_artifacts() {
        let qa = Path::new("/tmp/opencut-caption-qa");
        std::fs::create_dir_all(qa).unwrap();
        let cases = [
            (
                "mixed-cjk.png",
                "OpenCut 中文混排",
                "Arial",
                FontResolutionPolicy::ExplicitFallback {
                    families: vec!["Arial Unicode MS".into(), "Hiragino Sans GB".into()],
                },
                TextAlign::Center,
            ),
            (
                "arabic-rtl.png",
                "السلام عليكم OpenCut",
                "Geeza Pro",
                FontResolutionPolicy::ExplicitFallback {
                    families: vec!["Arial".into()],
                },
                TextAlign::Center,
            ),
            (
                "combining.png",
                "Cafe\u{301} nai\u{308}ve",
                "Arial",
                FontResolutionPolicy::Strict {},
                TextAlign::Center,
            ),
            (
                "multiline-left.png",
                "短行\nA much longer second line",
                "Arial Unicode MS",
                FontResolutionPolicy::Strict {},
                TextAlign::Left,
            ),
            (
                "multiline-center.png",
                "短行\nA much longer second line",
                "Arial Unicode MS",
                FontResolutionPolicy::Strict {},
                TextAlign::Center,
            ),
            (
                "multiline-right.png",
                "短行\nA much longer second line",
                "Arial Unicode MS",
                FontResolutionPolicy::Strict {},
                TextAlign::Right,
            ),
        ];
        for (filename, text, family, policy, align) in cases {
            let mut c = caption(text, family);
            c.font_policy = policy;
            c.text_align = align;
            c.max_width = Some(600);
            let report = render(&c, 800, 400, &qa.join(filename)).unwrap();
            assert!(report.pixel_bounds.width > 0 && report.pixel_bounds.height > 0);
        }
    }
}
