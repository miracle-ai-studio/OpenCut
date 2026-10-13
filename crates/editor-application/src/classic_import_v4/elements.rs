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
    res: &ClassicImportResourcesV4,
) -> Option<TrackV4> {
    let o = r.object(v, p)?;
    r.keys(o, p, &["id", "name", "type", "elements", "hidden", "muted"]);
    let kind = r.string(o, "type", p, None);
    let kind = match kind.as_str() {
        "video" => TrackKindV4::Video,
        "audio" => TrackKindV4::Audio,
        "text" => TrackKindV4::Text,
        _ => {
            r.block(
                &format!("{p}.type"),
                "UNSUPPORTED_TRACK",
                "This source track has no verified direct mapping",
            );
            return None;
        }
    };
    if kind == TrackKindV4::Audio && o.contains_key("hidden") {
        r.block(
            &format!("{p}.hidden"),
            "UNSUPPORTED_FIELD",
            "Audio tracks have no Classic hidden state",
        );
    }
    if kind == TrackKindV4::Text && o.contains_key("muted") {
        r.block(
            &format!("{p}.muted"),
            "UNSUPPORTED_FIELD",
            "Text tracks have no Classic mute state",
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
    Some(TrackV4 {
        id: r.string(o, "id", p, None),
        name: r.string(o, "name", p, None),
        kind,
        enabled: !r.boolean(o, "hidden", p, false),
        muted: r.boolean(o, "muted", p, false),
        locked: false,
        opacity: 1.,
        gain: GainV4::Decibels { value: 0. },
        elements,
        channels: vec![],
    })
}
fn rate(r: &mut Review, v: &Value, p: &str) -> RetimeV4 {
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
        return RetimeV4 {
            rate: RationalRateV4 {
                numerator: 1,
                denominator: 1,
            },
            maintain_pitch: pitch,
            mapping: RetimeMappingV4::ClassicRoundedTicks { timebase: 120000 },
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
    RetimeV4 {
        rate: RationalRateV4 {
            numerator: a,
            denominator: b,
        },
        maintain_pitch: pitch,
        mapping: RetimeMappingV4::ClassicRoundedTicks { timebase: 120000 },
    }
}
fn visual(r: &mut Review, o: &Map<String, Value>, p: &str) -> VisualStyleV4 {
    VisualStyleV4 {
        profile: VisualProfileV4::ClassicCanvasV31,
        transform: TransformV4 {
            position_x_px: r.num(o, "transform.positionX", p, 0.),
            position_y_px: r.num(o, "transform.positionY", p, 0.),
            scale_x: r.num(o, "transform.scaleX", p, 1.),
            scale_y: r.num(o, "transform.scaleY", p, 1.),
            rotation_degrees: r.num(o, "transform.rotate", p, 0.),
        },
        opacity: r.num(o, "opacity", p, 1.),
        blend_mode: r.enumeration(o, "blendMode", p, BlendModeV4::Normal),
        effects: vec![],
        masks: vec![],
    }
}
fn text(r: &mut Review, o: &Map<String, Value>, p: &str, res: &ClassicImportResourcesV4) -> TextV4 {
    let family = r.string(o, "fontFamily", p, Some("Arial"));
    let weight = r.enumeration(o, "fontWeight", p, FontWeightV4::Normal);
    let style = r.enumeration(o, "fontStyle", p, FontStyleV4::Normal);
    let request = ClassicFontRequestV4 {
        family: family.clone(),
        font_weight: weight,
        font_style: style,
    };
    let matches: Vec<_> = res.fonts.iter().filter(|f| f.request == request).collect();
    let font = if matches.len() == 1 {
        let f = &matches[0].font;
        if f.family != family
            || f.weight
                != match weight {
                    FontWeightV4::Normal => 400,
                    FontWeightV4::Bold => 700,
                }
            || f.style != style
        {
            r.block(
                p,
                "FONT_FACE_MISMATCH",
                "Resolved fixed font differs from explicit request",
            );
        }
        FontBindingV4::Locked {
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
        FontBindingV4::UnresolvedFamily {
            family,
            policy: FontResolutionPolicy::Strict {},
        }
    };
    TextV4 {
        content: r.string(o, "content", p, Some("Default text")),
        font,
        font_weight: weight,
        font_style: style,
        decoration: match o.get("textDecoration").and_then(Value::as_str) {
            Some("line-through") => TextDecorationV4::LineThrough,
            _ => r.enumeration(o, "textDecoration", p, TextDecorationV4::None),
        },
        align: r.enumeration(o, "textAlign", p, TextAlignV4::Center),
        font_size: FontSizeV4::ClassicHeight90 {
            value: r.num(o, "fontSize", p, 15.),
        },
        color: parse_color(r, o, "color", p, "#ffffff"),
        letter_spacing_px: r.num(o, "letterSpacing", p, 0.),
        line_height_multiplier: r.num(o, "lineHeight", p, 1.2),
        background: TextBackgroundV4 {
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
fn element(
    r: &mut Review,
    v: &Value,
    p: &str,
    res: &ClassicImportResourcesV4,
) -> Option<TimelineElementV4> {
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
    if !["text", "video", "audio", "image"].contains(&kind.as_str()) {
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
    for k in ["effects", "masks"] {
        if let Some(v) = o.get(k) {
            if !v.is_array() {
                r.block(
                    &format!("{p}.{k}"),
                    "INVALID_DEPENDENCY",
                    "Expected dependency array",
                );
            } else if v.as_array().is_some_and(|a| !a.is_empty()) {
                r.block(
                    &format!("{p}.{k}"),
                    "UNSUPPORTED_DEPENDENCY",
                    "Dependency cannot be silently dropped",
                );
            }
        }
    }
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
    if (kind == "text" || kind == "image") && o.contains_key("retime") {
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
    let mut allowed = if kind == "audio" {
        vec!["volume", "muted"]
    } else {
        VISUAL.to_vec()
    };
    if kind == "video" {
        allowed.extend(["volume", "muted"]);
    }
    if kind == "text" {
        allowed.extend(TEXT);
    }
    r.keys(params, &pp, &allowed);
    let id = r.string(o, "id", p, None);
    let content = if kind == "text" {
        ElementContentV4::Text {
            text: text(r, params, &pp, res),
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
        let audio = AudioStyleV4 {
            stream: None,
            gain: GainV4::Decibels {
                value: r.num(params, "volume", &pp, 0.),
            },
            muted: r.boolean(params, "muted", &pp, false),
        };
        match kind.as_str() {
            "video" => ElementContentV4::Video {
                asset_id,
                video_stream: None,
                source_audio_enabled: r.boolean(o, "isSourceAudioEnabled", p, true),
                audio,
                retime: rate(r, &v["retime"], &format!("{p}.retime")),
            },
            "audio" => ElementContentV4::Audio {
                asset_id,
                audio,
                retime: rate(r, &v["retime"], &format!("{p}.retime")),
            },
            _ => ElementContentV4::Image { asset_id },
        }
    };
    let e = TimelineElementV4 {
        id: id.clone(),
        name: r.string(o, "name", p, None),
        timeline_range: TimeRange {
            start: r.ticks(o, "startTime", p, None),
            duration: r.ticks(o, "duration", p, None),
        },
        source_span: ElementSourceSpanV4::ClassicTrimmed {
            trim_start: r.ticks(o, "trimStart", p, Some(0)),
            trim_end: r.ticks(o, "trimEnd", p, Some(0)),
            source_duration: o
                .get("sourceDuration")
                .map(|_| r.ticks(o, "sourceDuration", p, None)),
        },
        enabled: !r.boolean(o, "hidden", p, false),
        visual: if kind == "audio" {
            None
        } else {
            Some(visual(r, params, &pp))
        },
        channels: animation::parse(
            r,
            o.get("animations"),
            &format!("{p}.animations"),
            &id,
            &kind,
        ),
        content,
    };
    r.report.mapped.push(p.into());
    Some(e)
}
