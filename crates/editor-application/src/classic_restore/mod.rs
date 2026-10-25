//! Direct v31 parsing into the complete editor aggregate without projection through historical formats or I/O.
use opencut_api_contracts::classic_restore::*;
use opencut_contracts::{
    AssetKind, EditorError, FontResolutionPolicy, FrameRate, Time, TimeRange, scene::*,
};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
mod animation;
mod effects;
mod elements;
mod graphics;
mod legacy_shadows;
mod masks;
#[cfg(test)]
mod tests;
pub struct ClassicResources {
    pub media: BTreeMap<String, ProjectAsset>,
    pub fonts: Vec<ResolvedClassicFont>,
}
pub struct ResolvedClassicFont {
    pub request: ClassicFontRequest,
    pub font: FontResource,
}
pub struct PreparedClassicRestore {
    pub analysis: ClassicReview,
    pub document: Option<EditorDocument>,
}
pub trait ClassicResourceRead {
    fn resolve_classic_resources(
        &self,
        input: &AnalyzeClassic,
    ) -> Result<ClassicResources, EditorError>;
}
struct Review {
    report: ClassicReview,
}
impl Review {
    fn block(&mut self, p: &str, c: &str, d: &str) {
        self.report.blockers.push(CompatibilityFinding {
            path: p.into(),
            code: c.into(),
            detail: d.into(),
        });
    }
    fn loss(&mut self, p: &str, c: &str, d: &str) {
        self.report.losses.push(CompatibilityFinding {
            path: p.into(),
            code: c.into(),
            detail: d.into(),
        });
    }
    fn keys(&mut self, o: &Map<String, Value>, p: &str, allowed: &[&str]) {
        for k in o.keys() {
            if !allowed.contains(&k.as_str()) {
                self.block(
                    &format!("{p}.{k}"),
                    "UNSUPPORTED_FIELD",
                    "Field has no direct V4 mapping",
                );
            }
        }
    }
    fn object<'a>(&mut self, v: &'a Value, p: &str) -> Option<&'a Map<String, Value>> {
        let o = v.as_object();
        if o.is_none() {
            self.block(p, "INVALID_OBJECT", "Expected object");
        }
        o
    }
    fn string(
        &mut self,
        o: &Map<String, Value>,
        k: &str,
        p: &str,
        default: Option<&str>,
    ) -> String {
        match o.get(k) {
            Some(Value::String(v)) => v.clone(),
            None if default.is_some() => default.unwrap().into(),
            _ => {
                self.block(&format!("{p}.{k}"), "INVALID_STRING", "Expected string");
                String::new()
            }
        }
    }
    fn num(&mut self, o: &Map<String, Value>, k: &str, p: &str, d: f64) -> f64 {
        match o.get(k) {
            None => d,
            Some(v) => v.as_f64().filter(|x| x.is_finite()).unwrap_or_else(|| {
                self.block(
                    &format!("{p}.{k}"),
                    "INVALID_NUMBER",
                    "Expected finite number",
                );
                d
            }),
        }
    }
    fn boolean(&mut self, o: &Map<String, Value>, k: &str, p: &str, d: bool) -> bool {
        match o.get(k) {
            None => d,
            Some(v) => v.as_bool().unwrap_or_else(|| {
                self.block(&format!("{p}.{k}"), "INVALID_BOOLEAN", "Expected boolean");
                d
            }),
        }
    }
    fn ticks(&mut self, o: &Map<String, Value>, k: &str, p: &str, d: Option<i64>) -> Time {
        let n = match o.get(k) {
            None => d,
            Some(v) => v
                .as_i64()
                .filter(|n| n.unsigned_abs() <= 9_007_199_254_740_991),
        };
        if n.is_none() {
            self.block(
                &format!("{p}.{k}"),
                "INVALID_TIME",
                "Expected safe integer v31 ticks",
            );
        }
        t(n.unwrap_or(0))
    }
    fn enumeration<T: serde::de::DeserializeOwned>(
        &mut self,
        o: &Map<String, Value>,
        k: &str,
        p: &str,
        d: T,
    ) -> T {
        match o.get(k) {
            None => d,
            Some(v) => serde_json::from_value(v.clone()).unwrap_or_else(|_| {
                self.block(
                    &format!("{p}.{k}"),
                    "UNSUPPORTED_VALUE",
                    "Value has no typed V4 mapping",
                );
                d
            }),
        }
    }
}
fn t(n: i64) -> Time {
    Time {
        ticks: n.to_string(),
        timebase: 120000,
    }
}
fn color(s: &str) -> Option<Color> {
    let h = s.strip_prefix('#')?;
    let h = match h.len() {
        3 | 4 => h.chars().flat_map(|c| [c, c]).collect::<String>(),
        6 | 8 => h.into(),
        _ => return None,
    };
    let n = u32::from_str_radix(&h, 16).ok()?;
    let (rgb, a) = if h.len() == 8 {
        (n >> 8, (n & 255) as f64 / 255.)
    } else {
        (n, 1.)
    };
    Some(Color {
        encoding: ColorEncoding::Srgb,
        red: ((rgb >> 16) & 255) as f64 / 255.,
        green: ((rgb >> 8) & 255) as f64 / 255.,
        blue: (rgb & 255) as f64 / 255.,
        alpha: a,
    })
}
fn parse_color(r: &mut Review, o: &Map<String, Value>, k: &str, p: &str, d: &str) -> Color {
    let s = r.string(o, k, p, Some(d));
    color(&s).unwrap_or_else(|| {
        r.block(
            &format!("{p}.{k}"),
            "UNSUPPORTED_COLOR",
            "Only explicit hex sRGB colors have a verified mapping",
        );
        color(d).unwrap()
    })
}
fn canvas(r: &mut Review, v: &Value, p: &str) -> CanvasSize {
    let Some(o) = r.object(v, p) else {
        return CanvasSize {
            width: 1,
            height: 1,
        };
    };
    r.keys(o, p, &["width", "height"]);
    let mut dimension = |k: &str| {
        o.get(k)
            .and_then(Value::as_u64)
            .and_then(|v| u32::try_from(v).ok())
            .filter(|v| *v > 0)
            .unwrap_or_else(|| {
                r.block(
                    &format!("{p}.{k}"),
                    "INVALID_CANVAS",
                    "Expected positive integer dimension",
                );
                1
            })
    };
    CanvasSize {
        width: dimension("width"),
        height: dimension("height"),
    }
}
fn settings(r: &mut Review, v: &Value) -> Option<SettingsProfile> {
    let p = "$.settings";
    let o = r.object(v, p)?;
    r.keys(
        o,
        p,
        &[
            "fps",
            "canvasSize",
            "canvasSizeMode",
            "lastCustomCanvasSize",
            "originalCanvasSize",
            "background",
        ],
    );
    let fps: FrameRate = serde_json::from_value(o.get("fps").cloned().unwrap_or(Value::Null))
        .unwrap_or_else(|_| {
            r.block(
                "$.settings.fps",
                "INVALID_FPS",
                "Expected rational frame rate",
            );
            FrameRate {
                numerator: 30,
                denominator: 1,
            }
        });
    let size = canvas(r, &v["canvasSize"], "$.settings.canvasSize");
    let mut optional = |k: &str| {
        o.get(k)
            .filter(|v| !v.is_null())
            .map(|v| canvas(r, v, &format!("{p}.{k}")))
    };
    let last = optional("lastCustomCanvasSize");
    let original = optional("originalCanvasSize");
    let b = r.object(&v["background"], "$.settings.background")?;
    let background = match b.get("type").and_then(Value::as_str) {
        Some("color") => {
            r.keys(b, "$.settings.background", &["type", "color"]);
            Background::Color {
                color: parse_color(r, b, "color", "$.settings.background", "#000000"),
            }
        }
        Some("blur") => {
            r.keys(b, "$.settings.background", &["type", "blurIntensity"]);
            Background::MainTrackBlur {
                intensity: r.num(b, "blurIntensity", "$.settings.background", 10.),
            }
        }
        _ => {
            r.block(
                "$.settings.background.type",
                "UNSUPPORTED_BACKGROUND",
                "Unknown background",
            );
            return None;
        }
    };
    Some(SettingsProfile::ClassicGlobal {
        settings: ProjectSettings {
            fps,
            canvas_size: size,
            canvas_size_mode: r.enumeration(o, "canvasSizeMode", p, CanvasSizeMode::Preset),
            last_custom_canvas_size: last,
            original_canvas_size: original,
            background,
        },
    })
}
pub fn analyze(
    input: &AnalyzeClassic,
    resources: &ClassicResources,
) -> Result<PreparedClassicRestore, EditorError> {
    let original_content_id = source_content_id(&input.content);
    let (normalized, shadow_losses) = legacy_shadows::normalize(input);
    let input = &normalized;
    for resolved in &resources.fonts {
        crate::classic_fonts::validate_font_mapping(&resolved.request, &resolved.font)?;
    }
    let mut r = Review {
        report: ClassicReview {
            source_format: "classic-indexeddb-serialized-project".into(),
            source_version: None,
            source_content_id: original_content_id,
            target_project_id: input.target_project_id.clone(),
            mapped: vec![],
            losses: shadow_losses,
            blockers: vec![],
            unresolved_fonts: vec![],
            proposed_project: None,
        },
    };
    if input.content.len() > 1024 * 1024
        || input.media_mappings.len() > 256
        || input.font_mappings.len() > 256
        || input.target_project_id.trim().is_empty()
    {
        r.block(
            "$",
            "INVALID_INPUT",
            "Input exceeds bounded import envelope",
        );
        return Ok(PreparedClassicRestore {
            analysis: r.report,
            document: None,
        });
    }
    let raw: Value = match serde_json::from_str(&input.content) {
        Ok(v) => v,
        Err(_) => {
            r.block("$", "INVALID_JSON", "Invalid original Classic JSON");
            return Ok(PreparedClassicRestore {
                analysis: r.report,
                document: None,
            });
        }
    };
    let document = parse(&mut r, &raw, input, resources);
    let document = if r.report.blockers.is_empty() {
        document
    } else {
        None
    };
    if let Some(d) = &document {
        if let Err(e) = opencut_editor_core::scene::validate_project_scene(d) {
            r.block("$", "INVALID_V4_AGGREGATE", &e.message);
        }
    }
    let document = if r.report.blockers.is_empty() {
        document
    } else {
        None
    };
    if let Some(d) = &document {
        r.report.proposed_project = Some(
            crate::scene::public::project(&EditorSnapshot {
                project: d.clone(),
                revision: "analysis-only".into(),
            })
            .project,
        );
    }
    Ok(PreparedClassicRestore {
        analysis: r.report,
        document,
    })
}
fn parse(
    r: &mut Review,
    v: &Value,
    input: &AnalyzeClassic,
    res: &ClassicResources,
) -> Option<EditorDocument> {
    let o = r.object(v, "$")?;
    r.keys(
        o,
        "$",
        &[
            "version",
            "metadata",
            "settings",
            "scenes",
            "currentSceneId",
            "timelineViewState",
        ],
    );
    r.report.source_version = o
        .get("version")
        .and_then(Value::as_u64)
        .and_then(|x| u32::try_from(x).ok());
    if r.report.source_version != Some(31) {
        r.block(
            "$.version",
            "UNSUPPORTED_VERSION",
            "Requires migrated v31 source",
        );
    }
    let metadata = r.object(&v["metadata"], "$.metadata")?;
    r.keys(
        metadata,
        "$.metadata",
        &[
            "id",
            "name",
            "thumbnail",
            "duration",
            "createdAt",
            "updatedAt",
        ],
    );
    r.string(metadata, "id", "$.metadata", None);
    let name = r.string(metadata, "name", "$.metadata", None);
    for k in ["thumbnail", "duration", "createdAt", "updatedAt"] {
        if metadata.contains_key(k) {
            r.loss(
                &format!("$.metadata.{k}"),
                "METADATA_NOT_PORTED",
                "Presentation metadata is not revision-owned V4 content",
            );
        }
    }
    for k in ["currentSceneId", "timelineViewState"] {
        if o.contains_key(k) {
            r.loss(
                &format!("$.{k}"),
                "SESSION_STATE",
                "Session view is not revision-owned content",
            );
        }
    }
    let settings = settings(r, &v["settings"])?;
    let mut assets = vec![];
    for (id, a) in &res.media {
        let mut a = a.clone();
        a.id = crate::content_id("classic_asset", id.as_bytes());
        assets.push(a);
        r.report.mapped.push(format!("$.media_mappings.{id}"));
    }
    let mut fonts = vec![];
    for f in &res.fonts {
        if !fonts
            .iter()
            .any(|existing: &FontResource| existing.id == f.font.id)
        {
            fonts.push(f.font.clone());
        } else if !fonts.iter().any(|existing| existing == &f.font) {
            r.block(
                "$.font_mappings",
                "FONT_ID_CONFLICT",
                "Fixed font identity differs",
            );
        }
    }
    let scenes = match v["scenes"].as_array() {
        Some(v) => v,
        None => {
            r.block("$.scenes", "INVALID_SCENES", "Expected scenes array");
            return None;
        }
    };
    let mut sequences = vec![];
    let mut main = vec![];
    for (i, v) in scenes.iter().enumerate() {
        let p = format!("$.scenes[{i}]");
        let Some(o) = r.object(v, &p) else { continue };
        r.keys(
            o,
            &p,
            &[
                "id",
                "name",
                "isMain",
                "tracks",
                "bookmarks",
                "createdAt",
                "updatedAt",
            ],
        );
        let id = r.string(o, "id", &p, None);
        if r.boolean(o, "isMain", &p, false) {
            main.push(id.clone());
        }
        for k in ["createdAt", "updatedAt"] {
            if o.contains_key(k) {
                r.loss(
                    &format!("{p}.{k}"),
                    "METADATA_NOT_PORTED",
                    "Scene dates are not revision-owned content",
                );
            }
        }
        let Some(tracks) = r.object(&v["tracks"], &format!("{p}.tracks")) else {
            continue;
        };
        r.keys(
            tracks,
            &format!("{p}.tracks"),
            &["main", "overlay", "audio"],
        );
        let mut parsed = vec![];
        let Some(m) = elements::track(r, &v["tracks"]["main"], &format!("{p}.tracks.main"), res)
        else {
            continue;
        };
        let main_track_id = m.id.clone();
        parsed.push(m);
        // Classic overlay is stored top-to-bottom; V4 visual tracks are bottom-to-top.
        for group in ["overlay", "audio"] {
            let Some(list) = v["tracks"][group].as_array() else {
                r.block(
                    &format!("{p}.tracks.{group}"),
                    "INVALID_TRACKS",
                    "Expected track array",
                );
                continue;
            };
            let iter: Vec<usize> = if group == "overlay" {
                (0..list.len()).rev().collect()
            } else {
                (0..list.len()).collect()
            };
            for j in iter {
                if let Some(t) =
                    elements::track(r, &list[j], &format!("{p}.tracks.{group}[{j}]"), res)
                {
                    parsed.push(t);
                }
            }
        }
        let mut bookmarks = vec![];
        if let Some(list) = v["bookmarks"].as_array() {
            for (j, b) in list.iter().enumerate() {
                let bp = format!("{p}.bookmarks[{j}]");
                let Some(bo) = r.object(b, &bp) else { continue };
                r.keys(bo, &bp, &["time", "duration", "color", "note"]);
                bookmarks.push(opencut_contracts::scene_bookmarks::SceneBookmark {
                    id: crate::content_id("classic_bookmark", format!("{id}:{j}").as_bytes()),
                    sequence_id: id.clone(),
                    time: r.ticks(bo, "time", &bp, None),
                    duration: bo
                        .get("duration")
                        .map(|_| r.ticks(bo, "duration", &bp, None)),
                    color: bo.get("color").map(|_| r.string(bo, "color", &bp, None)),
                    note: bo.get("note").map(|_| r.string(bo, "note", &bp, None)),
                });
            }
        } else {
            r.block(
                &format!("{p}.bookmarks"),
                "INVALID_BOOKMARKS",
                "Expected bookmarks array",
            );
        }
        sequences.push(Scene {
            id,
            name: r.string(o, "name", &p, None),
            main_track_id,
            tracks: parsed,
            captions: vec![],
            transitions: vec![],
            bookmarks,
        });
    }
    if main.len() != 1 {
        r.block(
            "$.scenes",
            "INVALID_MAIN_SCENE",
            "Exactly one main scene required",
        );
    }
    Some(EditorDocument {
        schema_version: 4,
        id: input.target_project_id.clone(),
        name,
        default_sequence_id: main.first().cloned().unwrap_or_default(),
        settings,
        assets,
        fonts,
        sequences,
    })
}

/// Immutable identity always covers the exact original source bytes.
pub fn source_content_id(content: &str) -> String {
    crate::content_id("classic", content.as_bytes())
}
