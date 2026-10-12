//! Classic v31 SerializedProject conversion with explicit fidelity accounting.
use opencut_api_contracts::classic_import::{
    ClassicAnalysis, ClassicAnalyze, CompatibilityFinding,
};
use opencut_contracts::{
    Asset, AssetKind, CaptionCue, CaptionStyle, Clip, FontResolutionPolicy, FrameRate,
    ProjectDocument, Sequence, TextAlign, Time, TimeRange, Track, TrackKind,
};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};

const TICKS_PER_SECOND: u32 = 120_000;
const MAX_CLASSIC_BYTES: usize = 1024 * 1024;
const MAX_MEDIA_MAPPINGS: usize = 1000;

struct Review {
    report: ClassicAnalysis,
    assets: BTreeMap<String, Asset>,
    used_media: BTreeSet<String>,
}

impl Review {
    fn block(&mut self, path: &str, code: &str, detail: &str) {
        self.report.blockers.push(CompatibilityFinding {
            path: path.into(),
            code: code.into(),
            detail: detail.into(),
        });
    }
    fn loss(&mut self, path: &str, code: &str, detail: &str) {
        self.report.losses.push(CompatibilityFinding {
            path: path.into(),
            code: code.into(),
            detail: detail.into(),
        });
    }
    fn mapped(&mut self, path: &str) {
        self.report.mapped.push(path.into());
    }
    fn keys(&mut self, obj: &Map<String, Value>, path: &str, allowed: &[&str]) {
        for key in obj.keys().filter(|key| !allowed.contains(&key.as_str())) {
            self.block(
                &format!("{path}.{key}"),
                "UNSUPPORTED_FIELD",
                "Field has no verified Classic mapping",
            );
        }
    }
    fn media(&mut self, id: &str, path: &str, kind: AssetKind) -> Option<String> {
        let Some(source) = self.assets.get(id).cloned() else {
            self.block(
                path,
                "MEDIA_MAPPING_REQUIRED",
                "Classic media needs a fixed, host-verified AssetVersion mapping",
            );
            return None;
        };
        if source.kind != kind || source.version.is_none() {
            self.block(
                path,
                "MEDIA_KIND_OR_VERSION",
                "Mapped media kind/version differs from Classic element",
            );
            return None;
        }
        self.used_media.insert(id.into());
        Some(crate::content_id("classic_asset", id.as_bytes()))
    }
}

fn obj(value: &Value) -> Option<&Map<String, Value>> {
    value.as_object()
}
fn field<'a>(value: &'a Map<String, Value>, name: &str) -> Option<&'a str> {
    value.get(name)?.as_str()
}
fn ticks(value: Option<&Value>) -> Option<i64> {
    let value = value?.as_i64()?;
    (value.unsigned_abs() <= 9_007_199_254_740_991).then_some(value)
}
fn time(value: i64) -> Time {
    Time {
        ticks: value.to_string(),
        timebase: TICKS_PER_SECOND,
    }
}
fn range(start: i64, duration: i64) -> TimeRange {
    TimeRange {
        start: time(start),
        duration: time(duration),
    }
}
fn number(value: &Value) -> Option<f64> {
    value.as_f64().filter(|n| n.is_finite())
}

fn params(
    value: Option<&Value>,
    kind: &str,
    path: &str,
    review: &mut Review,
) -> BTreeMap<String, Value> {
    let Some(map) = value.and_then(obj) else {
        review.block(
            path,
            "INVALID_PARAMS",
            "Classic element params are missing or invalid",
        );
        return BTreeMap::new();
    };
    let mut converted = BTreeMap::new();
    let muted = map.get("muted").and_then(Value::as_bool) == Some(true);
    for (key, value) in map {
        let target = match key.as_str() {
            "transform.positionX" if kind != "audio" => Some("position_x"),
            "transform.positionY" if kind != "audio" => Some("position_y"),
            "transform.scaleX" if kind != "audio" => Some("scale_x"),
            "transform.scaleY" if kind != "audio" => Some("scale_y"),
            "transform.rotate" if kind != "audio" => Some("rotation_degrees"),
            "opacity" if kind != "audio" => Some("opacity"),
            _ => None,
        };
        if let Some(target) = target {
            if let Some(n) = number(value) {
                converted.insert(target.into(), json!(n));
            } else {
                review.block(
                    &format!("{path}.{key}"),
                    "INVALID_PARAM",
                    "Expected finite number",
                );
            }
            continue;
        }
        if key == "volume" && kind != "image" {
            if let Some(db) = number(value) {
                let gain = 10_f64.powf(db / 20.0);
                if gain <= 4.0 {
                    converted.insert("volume".into(), json!(gain));
                } else {
                    review.block(
                        &format!("{path}.{key}"),
                        "VOLUME_UNSUPPORTED",
                        "Classic dB gain exceeds OpenCut linear range",
                    );
                }
            } else {
                review.block(
                    &format!("{path}.{key}"),
                    "INVALID_PARAM",
                    "Expected finite dB number",
                );
            }
        } else if key == "muted" && kind != "image" {
            match value.as_bool() {
                Some(true) => {}
                Some(false) => {}
                None => review.block(
                    &format!("{path}.{key}"),
                    "INVALID_PARAM",
                    "Expected boolean mute",
                ),
            }
        } else if key == "blendMode" && value == "normal" {
            // Both renderers use normal alpha composition as their default.
        } else {
            review.block(
                &format!("{path}.{key}"),
                "UNSUPPORTED_PARAM",
                "Classic parameter has no equivalent evaluated by this renderer",
            );
        }
    }
    if muted && kind != "image" {
        converted.insert("volume".into(), json!(0.0));
    }
    converted
}

fn parse_element(value: &Value, path: &str, review: &mut Review) -> Option<(String, Clip)> {
    let Some(item) = obj(value) else {
        review.block(path, "INVALID_ELEMENT", "Expected an object");
        return None;
    };
    review.keys(
        item,
        path,
        &[
            "id",
            "name",
            "type",
            "duration",
            "startTime",
            "trimStart",
            "trimEnd",
            "sourceDuration",
            "params",
            "mediaId",
            "sourceType",
            "sourceUrl",
            "buffer",
            "retime",
            "isSourceAudioEnabled",
            "hidden",
            "effects",
            "masks",
            "animations",
        ],
    );
    let Some(kind) = field(item, "type") else {
        review.block(
            &format!("{path}.type"),
            "INVALID_ELEMENT",
            "Element type required",
        );
        return None;
    };
    if !matches!(kind, "video" | "audio" | "image") {
        review.block(
            path,
            "UNSUPPORTED_ELEMENT",
            "Only video, audio and image elements have clip mappings",
        );
        return None;
    }
    for dependency in ["effects", "masks", "animations"] {
        if item
            .get(dependency)
            .is_some_and(|v| !v.is_null() && v != &json!([]) && v != &json!({}))
        {
            review.block(
                &format!("{path}.{dependency}"),
                "UNSUPPORTED_DEPENDENCY",
                "Effect, mask or animation cannot be silently dropped",
            );
        }
    }
    if item.get("buffer").is_some_and(|v| !v.is_null()) {
        review.block(
            &format!("{path}.buffer"),
            "UNSUPPORTED_BUFFER",
            "Audio buffer is not a portable media reference",
        );
    }
    if item.get("sourceType").is_some_and(|v| v != "upload") {
        review.block(
            &format!("{path}.sourceType"),
            "UNSUPPORTED_SOURCE",
            "Library URL requires explicit media import before conversion",
        );
    }
    if item.get("sourceUrl").is_some() {
        review.block(
            &format!("{path}.sourceUrl"),
            "UNSUPPORTED_SOURCE",
            "External URL has no fixed content version",
        );
    }
    let Some(id) = field(item, "id").filter(|v| !v.is_empty()) else {
        review.block(&format!("{path}.id"), "INVALID_ID", "Element ID required");
        return None;
    };
    let Some(start) = ticks(item.get("startTime")) else {
        review.block(
            &format!("{path}.startTime"),
            "INVALID_TIME",
            "Expected integer v31 tick count",
        );
        return None;
    };
    let Some(duration) = ticks(item.get("duration")).filter(|n| *n > 0) else {
        review.block(
            &format!("{path}.duration"),
            "INVALID_TIME",
            "Expected positive integer v31 tick count",
        );
        return None;
    };
    for field in ["trimStart", "trimEnd", "sourceDuration"] {
        if item.get(field).is_some_and(|v| ticks(Some(v)).is_none()) {
            review.block(
                &format!("{path}.{field}"),
                "INVALID_TIME",
                "Expected integer v31 tick count",
            );
            return None;
        }
    }
    let trim_start = ticks(item.get("trimStart")).unwrap_or(0);
    let trim_end = ticks(item.get("trimEnd")).unwrap_or(0);
    if trim_start < 0 || trim_end < 0 {
        review.block(path, "INVALID_TRIM", "Classic trims must be nonnegative");
        return None;
    }
    let media_id = field(item, "mediaId").filter(|v| !v.is_empty());
    let Some(media_id) = media_id else {
        review.block(
            &format!("{path}.mediaId"),
            "MEDIA_MAPPING_REQUIRED",
            "Element lacks a Classic media ID",
        );
        return None;
    };
    let asset_kind = match kind {
        "video" => AssetKind::Video,
        "audio" => AssetKind::Audio,
        _ => AssetKind::Image,
    };
    let asset_id = review.media(media_id, &format!("{path}.mediaId"), asset_kind)?;
    let mut rate = 1.0_f64;
    if let Some(retime) = item.get("retime") {
        let Some(retime) = obj(retime) else {
            review.block(
                &format!("{path}.retime"),
                "INVALID_RETIME",
                "Expected retime object",
            );
            return None;
        };
        review.keys(
            retime,
            &format!("{path}.retime"),
            &["rate", "maintainPitch"],
        );
        if retime
            .get("rate")
            .is_some_and(|value| number(value).is_none())
            || retime
                .get("maintainPitch")
                .is_some_and(|value| value.as_bool().is_none())
        {
            review.block(
                &format!("{path}.retime"),
                "INVALID_RETIME",
                "Rate or pitch flag has invalid type",
            );
            return None;
        }
        rate = retime.get("rate").and_then(number).unwrap_or(1.0);
        if rate <= 0.0 {
            review.block(
                &format!("{path}.retime.rate"),
                "INVALID_RETIME",
                "Rate must be positive",
            );
            return None;
        }
        if retime.get("maintainPitch").and_then(Value::as_bool) == Some(true) && rate != 1.0 {
            review.block(
                &format!("{path}.retime.maintainPitch"),
                "UNSUPPORTED_PITCH",
                "Pitch preservation is not represented by source/timeline span ratio",
            );
        }
    }
    let source_range = if kind == "image" {
        if trim_start != 0 || trim_end != 0 || rate != 1.0 {
            review.block(
                path,
                "UNSUPPORTED_IMAGE_TIMING",
                "Image trim or retime has no source-span mapping",
            );
        }
        None
    } else {
        let scaled = (duration as f64) * rate;
        if !scaled.is_finite() || scaled.fract() != 0.0 || scaled > i64::MAX as f64 {
            review.block(
                &format!("{path}.retime"),
                "INEXACT_RETIME",
                "Rate does not produce an exact v31 source tick span",
            );
            return None;
        }
        let source_duration = scaled as i64;
        if let Some(total) = ticks(item.get("sourceDuration")) {
            if total
                .checked_sub(trim_start)
                .and_then(|n| n.checked_sub(trim_end))
                != Some(source_duration)
            {
                review.block(
                    path,
                    "TRIM_RETIME_MISMATCH",
                    "Classic source duration, trims and rate disagree",
                );
            }
        } else if trim_end != 0 {
            review.block(
                &format!("{path}.trimEnd"),
                "SOURCE_DURATION_REQUIRED",
                "Cannot verify end trim without sourceDuration",
            );
        }
        Some(range(trim_start, source_duration))
    };
    let mut converted = params(item.get("params"), kind, &format!("{path}.params"), review);
    if item.get("hidden").and_then(Value::as_bool) == Some(true) {
        if kind == "audio" {
            converted.insert("volume".into(), json!(0.0));
        } else {
            converted.insert("opacity".into(), json!(0.0));
        }
    }
    if kind == "video" && item.get("isSourceAudioEnabled").and_then(Value::as_bool) == Some(false) {
        converted.insert("volume".into(), json!(0.0));
    }
    if item.get("name").is_some() {
        review.loss(
            &format!("{path}.name"),
            "ELEMENT_NAME",
            "Clip display names are not stored in the new clip model",
        );
    }
    Some((
        kind.into(),
        Clip {
            id: id.into(),
            asset_id: Some(asset_id),
            timeline_range: range(start, duration),
            source_range,
            params: converted,
            channels: Vec::new(),
        },
    ))
}

fn parse_text(value: &Value, path: &str, review: &mut Review) -> Option<CaptionCue> {
    let Some(item) = obj(value) else {
        review.block(path, "INVALID_TEXT", "Expected text element");
        return None;
    };
    review.keys(
        item,
        path,
        &[
            "id",
            "name",
            "type",
            "duration",
            "startTime",
            "trimStart",
            "trimEnd",
            "params",
            "hidden",
            "effects",
            "animations",
        ],
    );
    if field(item, "type") != Some("text") {
        review.block(
            &format!("{path}.type"),
            "INVALID_TEXT",
            "Expected Classic text element",
        );
        return None;
    }
    for field in ["effects", "animations"] {
        if item
            .get(field)
            .is_some_and(|v| !v.is_null() && v != &json!([]) && v != &json!({}))
        {
            review.block(
                &format!("{path}.{field}"),
                "UNSUPPORTED_DEPENDENCY",
                "Text effect/animation has no caption mapping",
            );
        }
    }
    if item.get("hidden").and_then(Value::as_bool) == Some(true) {
        review.block(
            &format!("{path}.hidden"),
            "UNSUPPORTED_HIDDEN_TEXT",
            "Hidden text cannot be a visible caption",
        );
    }
    let Some(id) = field(item, "id") else {
        review.block(&format!("{path}.id"), "INVALID_ID", "Text ID required");
        return None;
    };
    let Some(start) = ticks(item.get("startTime")) else {
        review.block(path, "INVALID_TIME", "Text start must be integer ticks");
        return None;
    };
    let Some(duration) = ticks(item.get("duration")).filter(|n| *n > 0) else {
        review.block(path, "INVALID_TIME", "Text duration must be positive ticks");
        return None;
    };
    if ["trimStart", "trimEnd"].iter().any(|key| {
        item.get(*key)
            .is_some_and(|value| ticks(Some(value)).is_none())
    }) {
        review.block(path, "INVALID_TIME", "Text trim must be integer ticks");
        return None;
    }
    if ticks(item.get("trimStart")).unwrap_or(0) != 0
        || ticks(item.get("trimEnd")).unwrap_or(0) != 0
    {
        review.block(
            path,
            "UNSUPPORTED_TEXT_TRIM",
            "Text trim has no caption mapping",
        );
    }
    let Some(p) = item.get("params").and_then(obj) else {
        review.block(path, "INVALID_TEXT", "Text params required");
        return None;
    };
    let allowed = [
        "content",
        "fontFamily",
        "fontSize",
        "color",
        "textAlign",
        "fontWeight",
        "fontStyle",
        "textDecoration",
        "letterSpacing",
        "lineHeight",
        "background.enabled",
        "background.color",
        "background.cornerRadius",
        "background.paddingX",
        "background.paddingY",
        "background.offsetX",
        "background.offsetY",
        "transform.positionX",
        "transform.positionY",
        "transform.scaleX",
        "transform.scaleY",
        "transform.rotate",
        "opacity",
        "blendMode",
    ];
    review.keys(p, &format!("{path}.params"), &allowed);
    for (key, default) in [
        ("fontWeight", json!("normal")),
        ("fontStyle", json!("normal")),
        ("textDecoration", json!("none")),
        ("letterSpacing", json!(0)),
        ("background.enabled", json!(false)),
        ("transform.positionX", json!(0)),
        ("transform.positionY", json!(0)),
        ("transform.scaleX", json!(1)),
        ("transform.scaleY", json!(1)),
        ("transform.rotate", json!(0)),
        ("opacity", json!(1)),
        ("blendMode", json!("normal")),
    ] {
        if p.get(key).is_some_and(|value| value != &default) {
            review.block(
                &format!("{path}.params.{key}"),
                "TEXT_STYLE_UNSUPPORTED",
                "Text field has no faithful caption renderer mapping",
            );
        }
    }
    let Some(content) = field(p, "content").filter(|v| !v.trim().is_empty()) else {
        review.block(path, "INVALID_TEXT", "Text content required");
        return None;
    };
    if p.get("fontFamily").is_some_and(|v| v.as_str().is_none())
        || p.get("fontSize").is_some_and(|v| number(v).is_none())
        || p.get("color").is_some_and(|v| v.as_str().is_none())
        || p.get("lineHeight").is_some_and(|v| number(v).is_none())
    {
        review.block(
            &format!("{path}.params"),
            "INVALID_TEXT_STYLE",
            "Font, colour or line height has invalid type",
        );
        return None;
    }
    let font_family = field(p, "fontFamily").unwrap_or("Arial").to_string();
    let font_size = p.get("fontSize").and_then(number).unwrap_or(15.0);
    let color = field(p, "color").unwrap_or("#ffffff").to_string();
    let text_align = match field(p, "textAlign").unwrap_or("center") {
        "left" => TextAlign::Left,
        "center" => TextAlign::Center,
        "right" => TextAlign::Right,
        _ => {
            review.block(path, "INVALID_TEXT_ALIGN", "Unsupported text alignment");
            return None;
        }
    };
    let line_height = p.get("lineHeight").and_then(number).unwrap_or(1.2);
    if line_height <= 0.0 {
        review.block(
            path,
            "INVALID_LINE_HEIGHT",
            "Text line height must be positive",
        );
        return None;
    }
    review.loss(path,"TEXT_LAYOUT","Classic canvas-centered text becomes caption layout; inspect the render before accepting migration");
    Some(CaptionCue {
        id: id.into(),
        timeline_range: range(start, duration),
        text: content.into(),
        source_anchor: None,
        style: CaptionStyle {
            font_family,
            font_size,
            color,
            background_color: None,
            font_policy: FontResolutionPolicy::Strict {},
            max_width: None,
            line_height: Some((font_size * line_height) as f32),
            text_align,
        },
    })
}

fn parse_track(value: &Value, path: &str, review: &mut Review, sequence: &mut Sequence) {
    let Some(track) = obj(value) else {
        review.block(path, "INVALID_TRACK", "Track must be an object");
        return;
    };
    review.keys(
        track,
        path,
        &["id", "name", "type", "elements", "muted", "hidden"],
    );
    let Some(id) = field(track, "id").filter(|s| !s.is_empty()) else {
        review.block(path, "INVALID_TRACK", "Track ID required");
        return;
    };
    let kind = field(track, "type").unwrap_or("");
    if !matches!(kind, "video" | "audio" | "text") {
        review.block(
            &format!("{path}.type"),
            "UNSUPPORTED_TRACK",
            "Graphic/effect track has no verified mapping",
        );
        return;
    }
    let Some(elements) = track.get("elements").and_then(Value::as_array) else {
        review.block(path, "INVALID_TRACK", "Track elements array required");
        return;
    };
    if kind == "text" {
        if track.get("hidden").and_then(Value::as_bool) == Some(true) && !elements.is_empty() {
            review.block(
                &format!("{path}.hidden"),
                "UNSUPPORTED_HIDDEN_TEXT",
                "Hidden Classic text track cannot become visible caption cues",
            );
        }
        for (index, element) in elements.iter().enumerate() {
            if let Some(cue) = parse_text(element, &format!("{path}.elements[{index}]"), review) {
                sequence.caption_cues.push(cue);
            }
        }
        review.loss(
            path,
            "TEXT_TRACK",
            "Classic text track grouping is represented as sequence caption cues",
        );
        return;
    }
    let mut clips = Vec::new();
    let mut kinds = BTreeSet::new();
    for (index, element) in elements.iter().enumerate() {
        if let Some((element_kind, clip)) =
            parse_element(element, &format!("{path}.elements[{index}]"), review)
        {
            kinds.insert(element_kind);
            clips.push(clip);
        }
    }
    if kind == "audio" && kinds.iter().any(|item| item != "audio") {
        review.block(
            path,
            "TRACK_KIND_MISMATCH",
            "Audio track contains non-audio element",
        );
    }
    if kind == "video" && (kinds.contains("audio") || kinds.len() > 1) {
        review.block(
            path,
            "MIXED_VIDEO_IMAGE",
            "Mixed video/image Classic track needs explicit layout conversion",
        );
    }
    let mapped_kind = if kind == "audio" {
        TrackKind::Audio
    } else if kinds.contains("image") {
        TrackKind::Graphic
    } else {
        TrackKind::Video
    };
    let enabled = track.get("hidden").and_then(Value::as_bool) != Some(true);
    let muted = track.get("muted").and_then(Value::as_bool) == Some(true);
    sequence.tracks.push(Track {
        id: id.into(),
        name: field(track, "name").unwrap_or(id).into(),
        kind: mapped_kind,
        enabled,
        muted,
        locked: false,
        clips,
        params: BTreeMap::new(),
    });
    review.mapped(path);
}

/// Recovery keeps the complete mapped library, including unreferenced assets.
/// Legacy `analyze` remains unchanged: its unused mappings are explicit losses.
pub fn analyze_recovery(
    input: &ClassicAnalyze,
    media: &BTreeMap<String, Asset>,
) -> ClassicAnalysis {
    let (source, shadow_fields) = recovery_source(input);
    let mut report = analyze(&source, media);
    // The source identity always covers the actual supplied snapshot, including
    // explicitly reviewed legacy shadow fields, never just its cleaned projection.
    report.source_content_id = source_content_id(&input.content);
    report.losses.extend(shadow_fields);
    let Some(project) = report.proposed_project.as_mut() else {
        return report;
    };
    for (id, source) in media {
        let asset_id = crate::content_id("classic_asset", id.as_bytes());
        if project.assets.iter().any(|asset| asset.id == asset_id) {
            continue;
        }
        let mut asset = source.clone();
        asset.id = asset_id;
        if let Some(version) = &asset.version {
            asset.uri = format!("opencut-package://sha256/{}", version.content_hash);
        }
        project.assets.push(asset);
        report.mapped.push(format!("$.media_mappings.{id}"));
    }
    report
        .losses
        .retain(|loss| loss.code != "UNUSED_MEDIA_MAPPING");
    if let Err(error) = opencut_editor_core::validate_project(project) {
        report.blockers.push(CompatibilityFinding {
            path: "$.media_mappings".into(),
            code: "TARGET_VALIDATION".into(),
            detail: error.message,
        });
        report.proposed_project = None;
    }
    report
}

fn recovery_source(input: &ClassicAnalyze) -> (ClassicAnalyze, Vec<CompatibilityFinding>) {
    if input.content.len() > MAX_CLASSIC_BYTES {
        return (input.clone(), Vec::new());
    }
    let Ok(mut raw) = serde_json::from_str::<Value>(&input.content) else {
        return (input.clone(), Vec::new());
    };
    let mut losses = Vec::new();
    if raw["version"] != json!(31) {
        return (input.clone(), losses);
    }
    if let Some(root) = raw.as_object_mut() {
        let metadata = root.get("metadata").and_then(Value::as_object).cloned();
        if let Some(metadata) = metadata {
            for name in ["id", "name", "thumbnail", "createdAt", "updatedAt"] {
                // A contradictory keyPath identity is an error, not a shadow.
                if name == "id" && root.get(name) != metadata.get(name) {
                    continue;
                }
                if metadata.contains_key(name) {
                    remove_shadow(root, name, "$", &mut losses);
                }
            }
        }
        if root.get("settings").is_some_and(Value::is_object) {
            for name in [
                "fps",
                "canvasSize",
                "background",
                "backgroundType",
                "backgroundColor",
                "blurIntensity",
            ] {
                remove_shadow(root, name, "$", &mut losses);
            }
        }
        // Root bookmarks were moved to individual scenes by Classic v1→v2.
        if root
            .get("scenes")
            .and_then(Value::as_array)
            .is_some_and(|scenes| scenes.iter().all(|scene| scene.get("bookmarks").is_some()))
        {
            remove_shadow(root, "bookmarks", "$", &mut losses);
        }
    }
    if let Some(scenes) = raw.get_mut("scenes").and_then(Value::as_array_mut) {
        for (scene_index, scene) in scenes.iter_mut().enumerate() {
            if let Some(tracks) = scene.get_mut("tracks").and_then(Value::as_object_mut) {
                for (category, value) in tracks {
                    if category == "main" {
                        recovery_track(
                            value,
                            &format!("$.scenes[{scene_index}].tracks.main"),
                            &mut losses,
                        );
                    } else if let Some(items) = value.as_array_mut() {
                        for (index, track) in items.iter_mut().enumerate() {
                            recovery_track(
                                track,
                                &format!("$.scenes[{scene_index}].tracks.{category}[{index}]"),
                                &mut losses,
                            );
                        }
                    }
                }
            }
        }
    }
    let mut source = input.clone();
    source.content = raw.to_string();
    (source, losses)
}

fn remove_shadow(
    object: &mut Map<String, Value>,
    name: &str,
    path: &str,
    losses: &mut Vec<CompatibilityFinding>,
) {
    if object.remove(name).is_some() {
        losses.push(CompatibilityFinding {path: format!("{path}.{name}"), code: "LEGACY_SHADOW_FIELD".into(),
            detail: "Classic v31 uses its canonical metadata/settings/params; this obsolete serialized shadow is excluded from the new document. The source snapshot is unchanged.".into()});
    }
}

fn recovery_track(track: &mut Value, path: &str, losses: &mut Vec<CompatibilityFinding>) {
    let Some(elements) = track.get_mut("elements").and_then(Value::as_array_mut) else {
        return;
    };
    for (index, element) in elements.iter_mut().enumerate() {
        let Some(item) = element.as_object_mut() else {
            continue;
        };
        let Some(params) = item.get("params").and_then(Value::as_object).cloned() else {
            continue;
        };
        let path = format!("{path}.elements[{index}]");
        for name in [
            "opacity",
            "blendMode",
            "volume",
            "muted",
            "content",
            "fontSize",
            "fontFamily",
            "color",
            "textAlign",
            "fontWeight",
            "fontStyle",
            "textDecoration",
            "letterSpacing",
            "lineHeight",
        ] {
            if params.contains_key(name) {
                remove_shadow(item, name, &path, losses);
            }
        }
        if let Some(transform) = item.get("transform").and_then(Value::as_object) {
            let represented = transform.iter().all(|(key, value)| match key.as_str() {
                "position" => value.as_object().is_some_and(|position| {
                    position
                        .iter()
                        .all(|(key, value)| matches!(key.as_str(), "x" | "y") && value.is_number())
                        && params.contains_key("transform.positionX")
                        && params.contains_key("transform.positionY")
                }),
                "scale" => {
                    value.is_number()
                        && params.contains_key("transform.scaleX")
                        && params.contains_key("transform.scaleY")
                }
                "scaleX" => value.is_number() && params.contains_key("transform.scaleX"),
                "scaleY" => value.is_number() && params.contains_key("transform.scaleY"),
                "rotate" => value.is_number() && params.contains_key("transform.rotate"),
                _ => false,
            });
            if represented {
                remove_shadow(item, "transform", &path, losses);
            }
        }
    }
}

/// Pure dry-run. The caller must resolve media mappings under authorization and
/// verify each AssetVersion before passing them here. No file or DB is touched.
pub fn analyze(input: &ClassicAnalyze, media: &BTreeMap<String, Asset>) -> ClassicAnalysis {
    let mut review = Review {
        report: ClassicAnalysis {
            source_format: "classic-indexeddb-serialized-project".into(),
            source_version: None,
            source_content_id: crate::content_id("classic", input.content.as_bytes()),
            target_project_id: input.target_project_id.clone(),
            mapped: Vec::new(),
            losses: Vec::new(),
            blockers: Vec::new(),
            proposed_project: None,
        },
        assets: media.clone(),
        used_media: BTreeSet::new(),
    };
    if input.content.len() > MAX_CLASSIC_BYTES
        || input.media_mappings.len() > MAX_MEDIA_MAPPINGS
        || input.target_project_id.trim().is_empty()
    {
        review.block(
            "$",
            "INVALID_INPUT",
            "Classic JSON is too large or target ID is empty",
        );
        return review.report;
    }
    let ids = input
        .media_mappings
        .iter()
        .map(|mapping| mapping.classic_media_id.as_str())
        .collect::<BTreeSet<_>>();
    if ids.len() != input.media_mappings.len()
        || ids.len() != media.len()
        || media.keys().any(|id| !ids.contains(id.as_str()))
    {
        review.block(
            "$.media_mappings",
            "MEDIA_MAPPING_SET",
            "Media mapping IDs must be unique and match host-resolved assets",
        );
    }
    for (id, asset) in media {
        if asset.version.is_none() {
            review.block(
                &format!("$.media_mappings.{id}"),
                "MEDIA_VERSION_REQUIRED",
                "Mapping needs a host-verified fixed AssetVersion",
            );
        }
    }
    let Ok(raw) = serde_json::from_str::<Value>(&input.content) else {
        review.block("$", "INVALID_JSON", "Classic record must be JSON");
        return review.report;
    };
    let Some(root) = obj(&raw) else {
        review.block("$", "INVALID_ROOT", "Classic root must be an object");
        return review.report;
    };
    review.keys(
        root,
        "$",
        &[
            "metadata",
            "scenes",
            "currentSceneId",
            "settings",
            "version",
            "timelineViewState",
        ],
    );
    review.report.source_version = root
        .get("version")
        .and_then(Value::as_u64)
        .and_then(|v| u32::try_from(v).ok());
    if review.report.source_version != Some(31) {
        review.block(
            "$.version",
            "UNSUPPORTED_VERSION",
            "Only fully migrated Classic v31 records are accepted",
        );
    }
    let Some(metadata) = root.get("metadata").and_then(obj) else {
        review.block("$.metadata", "INVALID_METADATA", "Metadata object required");
        return review.report;
    };
    review.keys(
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
    let name = field(metadata, "name").unwrap_or("");
    if name.trim().is_empty() {
        review.block("$.metadata.name", "INVALID_NAME", "Project name required");
    }
    if field(metadata, "id").is_none() {
        review.block("$.metadata.id", "INVALID_ID", "Classic source ID required");
    }
    for ignored in ["thumbnail", "duration", "createdAt", "updatedAt"] {
        if metadata.contains_key(ignored) {
            review.loss(
                &format!("$.metadata.{ignored}"),
                "METADATA_NOT_PORTED",
                "Presentation metadata is not in the current project document",
            );
        }
    }
    if root.get("timelineViewState").is_some() {
        review.loss(
            "$.timelineViewState",
            "SESSION_STATE",
            "Timeline viewport is not project content",
        );
    }
    if root.get("currentSceneId").is_some() {
        review.loss(
            "$.currentSceneId",
            "SESSION_STATE",
            "Active Classic scene is not the default export sequence",
        );
    }
    let Some(settings) = root.get("settings").and_then(obj) else {
        review.block("$.settings", "INVALID_SETTINGS", "Settings object required");
        return review.report;
    };
    review.keys(
        settings,
        "$.settings",
        &[
            "fps",
            "canvasSize",
            "canvasSizeMode",
            "lastCustomCanvasSize",
            "originalCanvasSize",
            "background",
        ],
    );
    for ignored in [
        "canvasSizeMode",
        "lastCustomCanvasSize",
        "originalCanvasSize",
    ] {
        if settings.contains_key(ignored) {
            review.loss(
                &format!("$.settings.{ignored}"),
                "UI_PRESET",
                "Canvas preset history is not project content",
            );
        }
    }
    let Some(canvas) = settings.get("canvasSize").and_then(obj) else {
        review.block(
            "$.settings.canvasSize",
            "INVALID_CANVAS",
            "Canvas dimensions required",
        );
        return review.report;
    };
    review.keys(canvas, "$.settings.canvasSize", &["width", "height"]);
    let width = canvas
        .get("width")
        .and_then(Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .unwrap_or(0);
    let height = canvas
        .get("height")
        .and_then(Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .unwrap_or(0);
    if width == 0 || height == 0 {
        review.block(
            "$.settings.canvasSize",
            "INVALID_CANVAS",
            "Positive dimensions required",
        );
    }
    let Some(fps_raw) = settings.get("fps").and_then(obj) else {
        review.block(
            "$.settings.fps",
            "INVALID_FPS",
            "Rational frame rate required",
        );
        return review.report;
    };
    review.keys(fps_raw, "$.settings.fps", &["numerator", "denominator"]);
    let fps = FrameRate {
        numerator: fps_raw
            .get("numerator")
            .and_then(Value::as_u64)
            .and_then(|v| u32::try_from(v).ok())
            .unwrap_or(0),
        denominator: fps_raw
            .get("denominator")
            .and_then(Value::as_u64)
            .and_then(|v| u32::try_from(v).ok())
            .unwrap_or(0),
    };
    if fps.numerator == 0 || fps.denominator == 0 {
        review.block(
            "$.settings.fps",
            "INVALID_FPS",
            "Positive rational rate required",
        );
    }
    if let Some(background) = settings.get("background") {
        if background != &json!({"type":"color","color":"#000000"}) {
            review.block(
                "$.settings.background",
                "BACKGROUND_UNSUPPORTED",
                "Background is not represented in current ProjectDocument",
            );
        } else {
            review.loss(
                "$.settings.background",
                "BACKGROUND_DEFAULT",
                "Classic black background relies on current renderer default",
            );
        }
    }
    let Some(scenes) = root.get("scenes").and_then(Value::as_array) else {
        review.block("$.scenes", "INVALID_SCENES", "Scenes array required");
        return review.report;
    };
    if scenes.is_empty() {
        review.block("$.scenes", "EMPTY_PROJECT", "At least one scene required");
    }
    let mut sequences = Vec::new();
    let mut mains = Vec::new();
    for (index, scene_value) in scenes.iter().enumerate() {
        let path = format!("$.scenes[{index}]");
        let Some(scene) = obj(scene_value) else {
            review.block(&path, "INVALID_SCENE", "Scene object required");
            continue;
        };
        review.keys(
            scene,
            &path,
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
        let Some(id) = field(scene, "id").filter(|v| !v.is_empty()) else {
            review.block(&format!("{path}.id"), "INVALID_SCENE", "Scene ID required");
            continue;
        };
        if scene.get("isMain").and_then(Value::as_bool) == Some(true) {
            mains.push(id.to_string());
        }
        for ignored in ["createdAt", "updatedAt"] {
            if scene.contains_key(ignored) {
                review.loss(
                    &format!("{path}.{ignored}"),
                    "METADATA_NOT_PORTED",
                    "Scene timestamp is not in current sequence model",
                );
            }
        }
        if scene
            .get("bookmarks")
            .and_then(Value::as_array)
            .is_some_and(|items| !items.is_empty())
        {
            review.block(
                &format!("{path}.bookmarks"),
                "BOOKMARKS_UNSUPPORTED",
                "Bookmarks have no current project mapping",
            );
        }
        let mut sequence = Sequence {
            id: id.into(),
            name: field(scene, "name").unwrap_or(id).into(),
            width,
            height,
            fps: fps.clone(),
            tracks: Vec::new(),
            caption_cues: Vec::new(),
        };
        let Some(tracks) = scene.get("tracks").and_then(obj) else {
            review.block(
                &format!("{path}.tracks"),
                "INVALID_TRACKS",
                "Classic tracks object required",
            );
            continue;
        };
        review.keys(
            tracks,
            &format!("{path}.tracks"),
            &["main", "overlay", "audio"],
        );
        if let Some(main) = tracks.get("main") {
            parse_track(
                main,
                &format!("{path}.tracks.main"),
                &mut review,
                &mut sequence,
            );
        } else {
            review.block(
                &format!("{path}.tracks.main"),
                "MAIN_TRACK_REQUIRED",
                "Classic scene needs main video track",
            );
        }
        for group in ["overlay", "audio"] {
            if let Some(items) = tracks.get(group).and_then(Value::as_array) {
                for (offset, track) in items.iter().enumerate() {
                    parse_track(
                        track,
                        &format!("{path}.tracks.{group}[{offset}]"),
                        &mut review,
                        &mut sequence,
                    );
                }
            } else {
                review.block(
                    &format!("{path}.tracks.{group}"),
                    "INVALID_TRACKS",
                    "Track group array required",
                );
            }
        }
        sequences.push(sequence);
        review.mapped(&path);
    }
    if mains.len() != 1 {
        review.block(
            "$.scenes[*].isMain",
            "MAIN_SCENE_AMBIGUOUS",
            "Exactly one main Classic scene is required",
        );
    }
    for id in media
        .keys()
        .filter(|id| !review.used_media.contains(*id))
        .cloned()
        .collect::<Vec<_>>()
    {
        review.loss(
            &format!("$.media_mappings.{id}"),
            "UNUSED_MEDIA_MAPPING",
            "Mapping was not referenced by Classic elements",
        );
    }
    let mut assets = Vec::new();
    for media_id in review.used_media.iter().cloned().collect::<Vec<_>>() {
        if let Some(source) = media.get(&media_id) {
            let mut asset = source.clone();
            asset.id = crate::content_id("classic_asset", media_id.as_bytes());
            if let Some(version) = &asset.version {
                asset.uri = format!("opencut-package://sha256/{}", version.content_hash);
            }
            assets.push(asset);
            review.mapped(&format!("$.media_mappings.{media_id}"));
        }
    }
    if review.report.blockers.is_empty() {
        let project = ProjectDocument {
            id: input.target_project_id.clone(),
            name: name.into(),
            schema_version: 2,
            default_sequence_id: mains[0].clone(),
            sequences,
            assets,
        };
        match opencut_editor_core::validate_project(&project) {
            Ok(()) => review.report.proposed_project = Some(project),
            Err(error) => review.block("$", "TARGET_VALIDATION", &error.message),
        }
    }
    review.report
}

pub fn source_content_id(content: &str) -> String {
    crate::content_id("classic", content.as_bytes())
}

pub fn document_revision(
    document: &ProjectDocument,
) -> Result<String, opencut_contracts::EditorError> {
    let json = crate::encode_json(document, "Classic imported project")?;
    Ok(crate::content_id("rev", json.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencut_api_contracts::classic_import::ClassicMediaMapping;
    use opencut_contracts::AssetVersion;

    fn asset(id: &str, kind: AssetKind, marker: char) -> Asset {
        let hash = marker.to_string().repeat(64);
        Asset {
            id: id.into(),
            name: id.into(),
            kind,
            uri: format!("/verified/{id}"),
            duration: Some(time(240000)),
            version: Some(AssetVersion {
                id: format!("av_{hash}"),
                content_hash: hash,
                size: 1234,
            }),
            metadata: BTreeMap::new(),
        }
    }

    fn source() -> Value {
        json!({"metadata":{"id":"classic","name":"Interview","createdAt":"2024-01-01T00:00:00Z"},
            "version":31,"currentSceneId":"scene","settings":{"fps":{"numerator":24,"denominator":1},
            "canvasSize":{"width":320,"height":180},"background":{"type":"color","color":"#000000"}},
            "scenes":[{"id":"scene","name":"Main","isMain":true,"bookmarks":[],
              "tracks":{"main":{"id":"main","name":"Main","type":"video","hidden":false,"muted":false,
                  "elements":[{"id":"video-clip","name":"Video","type":"video","mediaId":"mv",
                    "startTime":0,"duration":240000,"trimStart":0,"trimEnd":0,"sourceDuration":240000,
                    "params":{"transform.positionX":5,"transform.scaleX":1.2,"volume":-6}}]},
                "overlay":[{"id":"text","name":"Text","type":"text","hidden":false,
                    "elements":[{"id":"cue","type":"text","startTime":12000,"duration":120000,
                        "trimStart":0,"trimEnd":0,"params":{"content":"你好","fontFamily":"Arial",
                        "fontSize":24,"color":"#ffffff","textAlign":"center"}}]}],
                "audio":[{"id":"audio","name":"Audio","type":"audio","muted":false,
                    "elements":[{"id":"audio-clip","type":"audio","mediaId":"ma","sourceType":"upload",
                        "startTime":0,"duration":240000,"trimStart":0,"trimEnd":0,"sourceDuration":240000,
                        "params":{"volume":-6}}]}]}}]})
    }

    fn input(value: Value) -> ClassicAnalyze {
        ClassicAnalyze {
            content: value.to_string(),
            target_project_id: "restored".into(),
            media_mappings: vec![
                ClassicMediaMapping {
                    classic_media_id: "mv".into(),
                    source_project_id: "library".into(),
                    source_revision: "r".into(),
                    asset_id: "video".into(),
                },
                ClassicMediaMapping {
                    classic_media_id: "ma".into(),
                    source_project_id: "library".into(),
                    source_revision: "r".into(),
                    asset_id: "audio".into(),
                },
            ],
        }
    }

    fn media() -> BTreeMap<String, Asset> {
        BTreeMap::from([
            ("mv".into(), asset("video", AssetKind::Video, 'a')),
            ("ma".into(), asset("audio", AssetKind::Audio, 'b')),
        ])
    }

    #[test]
    fn recovery_preserves_unreferenced_library_assets_without_changing_legacy_analysis() {
        let mut input = input(source());
        let mut media = media();
        media.insert("unused".into(), asset("shelf-image", AssetKind::Image, 'c'));
        input.media_mappings.push(ClassicMediaMapping {
            classic_media_id: "unused".into(),
            source_project_id: "library".into(),
            source_revision: "r".into(),
            asset_id: "shelf-image".into(),
        });
        let legacy = analyze(&input, &media);
        let recovered = analyze_recovery(&input, &media);
        assert_eq!(legacy.proposed_project.as_ref().unwrap().assets.len(), 2);
        assert!(
            legacy
                .losses
                .iter()
                .any(|loss| loss.code == "UNUSED_MEDIA_MAPPING")
        );
        let project = recovered.proposed_project.as_ref().unwrap();
        assert_eq!(project.assets.len(), 3);
        let shelf = project
            .assets
            .iter()
            .find(|asset| asset.name == "shelf-image")
            .unwrap();
        assert_eq!(shelf.version, media["unused"].version);
        assert_eq!(
            shelf.uri,
            format!("opencut-package://sha256/{}", "c".repeat(64))
        );
        assert!(
            !recovered
                .losses
                .iter()
                .any(|loss| loss.code == "UNUSED_MEDIA_MAPPING")
        );
        assert_eq!(recovered.source_content_id, legacy.source_content_id);
        assert_eq!(
            project.sequences,
            legacy.proposed_project.unwrap().sequences
        );
    }

    #[test]
    fn recovery_does_not_publish_a_projection_for_unsupported_semantics_or_unpinned_shelf_media() {
        let mut source_data = source();
        source_data["scenes"][0]["tracks"]["main"]["elements"][0]["effects"] =
            json!([{"type":"unknown"}]);
        let report = analyze_recovery(&input(source_data), &media());
        assert!(report.proposed_project.is_none());
        assert!(
            report
                .blockers
                .iter()
                .any(|finding| finding.code == "UNSUPPORTED_DEPENDENCY")
        );
        let mut input = input(source());
        let mut media = media();
        let mut asset = asset("shelf", AssetKind::Image, 'c');
        asset.version = None;
        media.insert("unused".into(), asset);
        input.media_mappings.push(ClassicMediaMapping {
            classic_media_id: "unused".into(),
            source_project_id: "library".into(),
            source_revision: "r".into(),
            asset_id: "shelf".into(),
        });
        let report = analyze_recovery(&input, &media);
        assert!(report.proposed_project.is_none());
        assert!(
            report
                .blockers
                .iter()
                .any(|finding| finding.code == "MEDIA_VERSION_REQUIRED")
        );
    }

    #[test]
    fn recovery_reviews_obsolete_shadows_but_keeps_original_source_identity_and_canonical_values() {
        let mut raw = source();
        raw["id"] = json!("classic");
        raw["name"] = json!("obsolete title");
        raw["fps"] = json!(60);
        raw["scenes"][0]["tracks"]["main"]["elements"][0]["volume"] = json!(0);
        let input = input(raw);
        assert!(analyze(&input, &media()).proposed_project.is_none());
        let report = analyze_recovery(&input, &media());
        assert!(report.blockers.is_empty(), "{:?}", report.blockers);
        assert_eq!(report.source_content_id, source_content_id(&input.content));
        let shadows: Vec<_> = report
            .losses
            .iter()
            .filter(|f| f.code == "LEGACY_SHADOW_FIELD")
            .map(|f| f.path.as_str())
            .collect();
        assert_eq!(
            shadows,
            vec![
                "$.id",
                "$.name",
                "$.fps",
                "$.scenes[0].tracks.main.elements[0].volume"
            ]
        );
        let project = report.proposed_project.unwrap();
        assert_eq!(project.name, "Interview");
        assert!(
            (project.sequences[0].tracks[0].clips[0].params["volume"]
                .as_f64()
                .unwrap()
                - 0.5011872336272722)
                .abs()
                < 1e-12
        );
    }

    #[test]
    fn recovery_cannot_erase_conflicting_id_or_unknown_transform_dependencies() {
        for transform in [
            json!({"scale": {"curve": "unknown"}}),
            json!({"position": {"x": 0, "y": 0, "z": 1}}),
        ] {
            let mut raw = source();
            let clip = &mut raw["scenes"][0]["tracks"]["main"]["elements"][0];
            clip["transform"] = transform;
            clip["params"]["transform.scaleY"] = json!(1);
            clip["params"]["transform.positionY"] = json!(0);
            let report = analyze_recovery(&input(raw), &media());
            assert!(report.proposed_project.is_none());
            assert!(
                report
                    .blockers
                    .iter()
                    .any(|f| f.path.ends_with("transform"))
            );
        }
        let mut raw = source();
        raw["id"] = json!("different");
        let report = analyze_recovery(&input(raw), &media());
        assert!(report.proposed_project.is_none());
        assert!(report.blockers.iter().any(|f| f.path == "$.id"));
    }

    #[test]
    fn nonempty_video_audio_text_builds_valid_fixed_asset_document() {
        let report = analyze(&input(source()), &media());
        assert!(report.blockers.is_empty(), "{:?}", report.blockers);
        let document = report.proposed_project.unwrap();
        assert_eq!(document.assets.len(), 2);
        assert_eq!(document.sequences[0].tracks.len(), 2);
        assert_eq!(document.sequences[0].caption_cues[0].text, "你好");
        let clip = &document.sequences[0].tracks[0].clips[0];
        assert_eq!(clip.params["position_x"], 5.0);
        assert_eq!(clip.source_range.as_ref().unwrap().duration, time(240000));
        assert!(
            report
                .losses
                .iter()
                .any(|issue| issue.code == "TEXT_LAYOUT")
        );
    }

    #[test]
    fn effects_or_missing_media_block_candidate_with_exact_path() {
        let mut value = source();
        value["scenes"][0]["tracks"]["main"]["elements"][0]["effects"] = json!([{"type":"blur"}]);
        let report = analyze(&input(value), &media());
        assert!(report.proposed_project.is_none());
        assert!(
            report
                .blockers
                .iter()
                .any(|issue| issue.path.ends_with("effects"))
        );
        let mut sources = media();
        sources.remove("mv");
        let report = analyze(&input(source()), &sources);
        assert!(
            report
                .blockers
                .iter()
                .any(|issue| issue.code == "MEDIA_MAPPING_REQUIRED")
        );
    }
}
