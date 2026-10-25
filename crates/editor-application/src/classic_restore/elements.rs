use super::*;
const VISUAL: &[&str] = &[
    "transform.positionX",
    "transform.positionY",
    "transform.scaleX",
    "transform.scaleY",
    "transform.rotate",
    "opacity",
    "blendMode",
];
const TEXT: &[&str] = &[
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
];
pub(super) fn track(
    r: &mut Review,
    v: &Value,
    p: &str,
    res: &ClassicResources,
) -> Option<SceneTrack> {
    let o = r.object(v, p)?;
    r.keys(o, p, &["id", "name", "type", "elements", "hidden", "muted"]);
    let kind = r.string(o, "type", p, None);
    let kind = match kind.as_str() {
        "video" => SceneTrackKind::Video,
        "audio" => SceneTrackKind::Audio,
        "text" => SceneTrackKind::Text,
        "graphic" => SceneTrackKind::Graphic,
        "effect" => SceneTrackKind::Effect,
        _ => {
            r.block(
                &format!("{p}.type"),
                "UNSUPPORTED_TRACK",
                "This source track has no verified direct mapping",
            );
            return None;
        }
    };
    if kind == SceneTrackKind::Audio && o.contains_key("hidden") {
        r.block(
            &format!("{p}.hidden"),
            "UNSUPPORTED_FIELD",
            "Audio tracks have no Classic hidden state",
        );
    }
    if matches!(
        kind,
        SceneTrackKind::Text | SceneTrackKind::Graphic | SceneTrackKind::Effect
    ) && o.contains_key("muted")
    {
        r.block(
            &format!("{p}.muted"),
            "UNSUPPORTED_FIELD",
            "Text and graphic tracks have no Classic mute state",
        );
    }
    let mut elements = vec![];
    if let Some(list) = v["elements"].as_array() {
        for (i, v) in list.iter().enumerate() {
            if let Some(e) = element(r, v, &format!("{p}.elements[{i}]"), res) {
                elements.push(e);
            }
        }
    } else {
        r.block(
            &format!("{p}.elements"),
            "INVALID_ELEMENTS",
            "Expected elements array",
        );
    }
    r.report.mapped.push(p.into());
    Some(SceneTrack {
        id: r.string(o, "id", p, None),
        name: r.string(o, "name", p, None),
        kind,
        enabled: !r.boolean(o, "hidden", p, false),
        muted: r.boolean(o, "muted", p, false),
        locked: false,
        opacity: 1.,
        gain: Gain::Decibels { value: 0. },
        elements,
        channels: vec![],
    })
}
fn rate(r: &mut Review, v: &Value, p: &str) -> Retime {
    let mut rate = 1.;
    let mut pitch = false;
    if !v.is_null() {
        if let Some(o) = r.object(v, p) {
            r.keys(o, p, &["rate", "maintainPitch"]);
            rate = r.num(o, "rate", p, 1.);
            pitch = r.boolean(o, "maintainPitch", p, false);
        }
    }
    // Decimal rates are represented exactly by bounded integers, never inferred
    // from source/timeline ratios or approximated by a different speed.
    let mut factor = 1u64;
    let mut scaled = rate;
    while scaled.fract() != 0. && factor < 1_000_000_000 {
        factor *= 10;
        scaled = rate * factor as f64;
    }
    if rate <= 0. || scaled.fract() != 0. || scaled > u32::MAX as f64 || factor > u32::MAX as u64 {
        r.block(
            p,
            "UNREPRESENTABLE_RATE",
            "Rate cannot be stored as bounded exact rational",
        );
        return Retime {
            rate: RationalRate {
                numerator: 1,
                denominator: 1,
            },
            maintain_pitch: pitch,
            mapping: RetimeMapping::ClassicRoundedTicks { timebase: 120000 },
        };
    }
    let mut a = scaled as u32;
    let mut b = factor as u32;
    let (mut x, mut y) = (a, b);
    while y != 0 {
        let rem = x % y;
        x = y;
        y = rem;
    }
    a /= x;
    b /= x;
    Retime {
        rate: RationalRate {
            numerator: a,
            denominator: b,
        },
        maintain_pitch: pitch,
        mapping: RetimeMapping::ClassicRoundedTicks { timebase: 120000 },
    }
}
fn visual(r: &mut Review, o: &Map<String, Value>, p: &str) -> VisualStyle {
    VisualStyle {
        profile: VisualProfile::ClassicCanvasV31,
        transform: Transform {
            position_x_px: r.num(o, "transform.positionX", p, 0.),
            position_y_px: r.num(o, "transform.positionY", p, 0.),
            scale_x: r.num(o, "transform.scaleX", p, 1.),
            scale_y: r.num(o, "transform.scaleY", p, 1.),
            rotation_degrees: r.num(o, "transform.rotate", p, 0.),
        },
        opacity: r.num(o, "opacity", p, 1.),
        blend_mode: r.enumeration(o, "blendMode", p, BlendMode::Normal),
        effects: vec![],
        masks: vec![],
    }
}
pub(super) fn font_binding(
    r: &mut Review,
    family: String,
    weight: FontWeight,
    style: FontStyle,
    p: &str,
    res: &ClassicResources,
) -> FontBinding {
    let request = ClassicFontRequest {
        family: family.clone(),
        font_weight: weight,
        font_style: style,
    };
    let matches: Vec<_> = res.fonts.iter().filter(|f| f.request == request).collect();
    if matches.len() == 1 {
        let f = &matches[0].font;
        let matches = crate::classic_fonts::validate_font_mapping(&request, f).is_ok();
        if !matches {
            r.block(
                p,
                "FONT_FACE_MISMATCH",
                "Resolved fixed font differs from explicit request",
            );
        }
        FontBinding::Locked {
            font_id: f.id.clone(),
        }
    } else {
        if matches.len() > 1 {
            r.block(
                p,
                "DUPLICATE_FONT_MAPPING",
                "Repeated family/weight/style mapping",
            );
        }
        if !r.report.unresolved_fonts.contains(&request) {
            r.report.unresolved_fonts.push(request);
        }
        FontBinding::UnresolvedFamily {
            family,
            policy: FontResolutionPolicy::Strict {},
        }
    }
}
fn text(r: &mut Review, o: &Map<String, Value>, p: &str, res: &ClassicResources) -> Text {
    let family = r.string(o, "fontFamily", p, Some("Arial"));
    let weight = r.enumeration(o, "fontWeight", p, FontWeight::Normal);
    let style = r.enumeration(o, "fontStyle", p, FontStyle::Normal);
    let font = font_binding(r, family, weight, style, p, res);
    Text {
        content: r.string(o, "content", p, Some("Default text")),
        font,
        font_weight: weight,
        font_style: style,
        decoration: match o.get("textDecoration").and_then(Value::as_str) {
            Some("line-through") => TextDecoration::LineThrough,
            _ => r.enumeration(o, "textDecoration", p, TextDecoration::None),
        },
        align: r.enumeration(o, "textAlign", p, SceneTextAlign::Center),
        font_size: FontSize::ClassicHeight90 {
            value: r.num(o, "fontSize", p, 15.),
        },
        color: parse_color(r, o, "color", p, "#ffffff"),
        letter_spacing_px: r.num(o, "letterSpacing", p, 0.),
        line_height_multiplier: r.num(o, "lineHeight", p, 1.2),
        background: TextBackground {
            enabled: r.boolean(o, "background.enabled", p, false),
            color: parse_color(r, o, "background.color", p, "#000000"),
            corner_radius_percent: r.num(o, "background.cornerRadius", p, 0.),
            padding_x_at_size15: r.num(o, "background.paddingX", p, 30.),
            padding_y_at_size15: r.num(o, "background.paddingY", p, 42.),
            offset_x_px: r.num(o, "background.offsetX", p, 0.),
            offset_y_px: r.num(o, "background.offsetY", p, 0.),
        },
    }
}
fn element(r: &mut Review, v: &Value, p: &str, res: &ClassicResources) -> Option<TimelineElement> {
    let o = r.object(v, p)?;
    r.keys(
        o,
        p,
        &[
            "id",
            "name",
            "type",
            "duration",
            "startTime",
            "trimStart",
            "trimEnd",
            "sourceDuration",
            "animations",
            "params",
            "definitionId",
            "effectType",
            "mediaId",
            "sourceType",
            "sourceUrl",
            "buffer",
            "retime",
            "isSourceAudioEnabled",
            "hidden",
            "effects",
            "masks",
        ],
    );
    let kind = r.string(o, "type", p, None);
    if !["text", "video", "audio", "image", "graphic", "effect"].contains(&kind.as_str()) {
        r.block(
            &format!("{p}.type"),
            "UNSUPPORTED_ELEMENT",
            "No verified direct V4 element mapping",
        );
        return None;
    }
    if kind == "text" {
        for k in ["mediaId", "sourceType", "sourceUrl", "buffer", "masks"] {
            if o.contains_key(k) {
                r.block(
                    &format!("{p}.{k}"),
                    "UNSUPPORTED_FIELD",
                    "Field is not part of a v31 text element",
                );
            }
        }
    }
    if kind == "graphic" {
        for k in ["mediaId", "sourceType", "sourceUrl", "buffer"] {
            if o.contains_key(k) {
                r.block(
                    &format!("{p}.{k}"),
                    "UNSUPPORTED_FIELD",
                    "Field is not part of a v31 parametric graphic element",
                );
            }
        }
    } else if o.contains_key("definitionId") {
        r.block(
            &format!("{p}.definitionId"),
            "UNSUPPORTED_FIELD",
            "Graphic definition only belongs to a parametric graphic element",
        );
    }
    if kind != "effect" && o.contains_key("effectType") {
        r.block(
            &format!("{p}.effectType"),
            "UNSUPPORTED_FIELD",
            "Effect type only belongs to an adjustment layer",
        );
    }
    if kind == "effect" {
        for field in [
            "mediaId",
            "sourceType",
            "sourceUrl",
            "buffer",
            "effects",
            "masks",
            "retime",
            "hidden",
        ] {
            if o.contains_key(field) {
                r.block(
                    &format!("{p}.{field}"),
                    "UNSUPPORTED_FIELD",
                    "Field is not part of a Classic adjustment layer",
                );
            }
        }
    }
    let effects = effects::parse(r, o.get("effects"), &format!("{p}.effects"), &kind);
    let masks = masks::parse(r, o.get("masks"), &format!("{p}.masks"), &kind, res);
    if o.get("buffer").is_some_and(|v| !v.is_null()) {
        r.block(
            &format!("{p}.buffer"),
            "UNSUPPORTED_BUFFER",
            "Nonportable audio buffer",
        );
    }
    if o.contains_key("sourceUrl") {
        r.block(
            &format!("{p}.sourceUrl"),
            "UNSUPPORTED_SOURCE",
            "URL is not an immutable resource mapping",
        );
    }
    if o.get("sourceType").is_some_and(|v| v != "upload") {
        r.block(
            &format!("{p}.sourceType"),
            "UNSUPPORTED_SOURCE",
            "Library source must first be explicitly imported",
        );
    }
    if matches!(kind.as_str(), "text" | "image" | "graphic") && o.contains_key("retime") {
        r.block(
            &format!("{p}.retime"),
            "UNSUPPORTED_RETIME",
            "Generated content has no Classic retime",
        );
    }
    if kind != "video" && o.contains_key("isSourceAudioEnabled") {
        r.block(
            &format!("{p}.isSourceAudioEnabled"),
            "UNSUPPORTED_FIELD",
            "Source audio flag only belongs to video",
        );
    }
    let pp = format!("{p}.params");
    let params = r.object(&v["params"], &pp)?;
    let graphic_definition = if kind == "graphic" {
        Some(graphics::definition(r, o, p)?)
    } else {
        None
    };
    let mut allowed = if kind == "audio" {
        vec!["volume", "muted"]
    } else if kind == "effect" {
        vec!["intensity"]
    } else {
        VISUAL.to_vec()
    };
    if kind == "video" {
        allowed.extend(["volume", "muted"]);
    }
    if kind == "text" {
        allowed.extend(TEXT);
    }
    if let Some(definition) = graphic_definition {
        allowed.extend(definition.parameters());
    }
    r.keys(params, &pp, &allowed);
    let id = r.string(o, "id", p, None);
    let content = if kind == "effect" {
        let name = r.string(o, "effectType", p, None);
        ElementContent::EffectLayer {
            effect: SceneEffect {
                id: crate::content_id("classic_layer_effect", id.as_bytes()),
                enabled: true,
                effect: effects::kind(r, &name, params, &pp)?,
            },
        }
    } else if kind == "text" {
        ElementContent::Text {
            text: text(r, params, &pp, res),
        }
    } else if let Some(definition) = graphic_definition {
        ElementContent::Graphic {
            graphic: graphics::parse(r, params, &pp, definition),
        }
    } else {
        let media = r.string(o, "mediaId", p, None);
        let Some(asset) = res.media.get(&media) else {
            r.block(
                &format!("{p}.mediaId"),
                "MEDIA_MAPPING_REQUIRED",
                "No authorized fixed resource mapping",
            );
            return None;
        };
        let expected = match kind.as_str() {
            "video" => AssetKind::Video,
            "audio" => AssetKind::Audio,
            _ => AssetKind::Image,
        };
        if asset.kind != expected && !(kind == "audio" && asset.kind == AssetKind::Video) {
            r.block(
                &format!("{p}.mediaId"),
                "MEDIA_KIND_MISMATCH",
                "Mapped resource kind differs",
            );
        }
        let asset_id = crate::content_id("classic_asset", media.as_bytes());
        let audio = AudioStyle {
            stream: None,
            gain: Gain::Decibels {
                value: r.num(params, "volume", &pp, 0.),
            },
            muted: r.boolean(params, "muted", &pp, false),
        };
        match kind.as_str() {
            "video" => ElementContent::Video {
                asset_id,
                video_stream: None,
                source_audio_enabled: r.boolean(o, "isSourceAudioEnabled", p, true),
                audio,
                retime: rate(r, &v["retime"], &format!("{p}.retime")),
            },
            "audio" => ElementContent::Audio {
                asset_id,
                audio,
                retime: rate(r, &v["retime"], &format!("{p}.retime")),
            },
            _ => ElementContent::Image { asset_id },
        }
    };
    let e = TimelineElement {
        id: id.clone(),
        name: r.string(o, "name", p, None),
        timeline_range: TimeRange {
            start: r.ticks(o, "startTime", p, None),
            duration: r.ticks(o, "duration", p, None),
        },
        source_span: ElementSourceSpan::ClassicTrimmed {
            trim_start: r.ticks(o, "trimStart", p, Some(0)),
            trim_end: r.ticks(o, "trimEnd", p, Some(0)),
            source_duration: o
                .get("sourceDuration")
                .map(|_| r.ticks(o, "sourceDuration", p, None)),
        },
        enabled: !r.boolean(o, "hidden", p, false),
        visual: if matches!(kind.as_str(), "audio" | "effect") {
            None
        } else {
            Some(VisualStyle {
                effects: effects.clone(),
                masks,
                ..visual(r, params, &pp)
            })
        },
        channels: animation::parse(
            r,
            o.get("animations"),
            &format!("{p}.animations"),
            &id,
            &kind,
            graphic_definition,
            &effects,
            match &content {
                ElementContent::Graphic { graphic } => graphic.stroke.align,
                _ => StrokeAlign::Center,
            },
        ),
        content,
    };
    r.report.mapped.push(p.into());
    Some(e)
}
