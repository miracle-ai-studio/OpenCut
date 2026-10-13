//! Preserve actual Classic curve controls and local clocks, never sample curves.
use super::*;
fn binding(path: &str, kind: &str) -> Option<ScalarBindingV4> {
    use ScalarParameterV4::*;
    let p = match path {
        "transform.positionX" if kind != "audio" => PositionXPx,
        "transform.positionY" if kind != "audio" => PositionYPx,
        "transform.scaleX" if kind != "audio" => ScaleX,
        "transform.scaleY" if kind != "audio" => ScaleY,
        "transform.rotate" if kind != "audio" => RotationDegrees,
        "opacity" if kind != "audio" => Opacity,
        "volume" if kind == "audio" || kind == "video" => GainDecibels,
        "fontSize" if kind == "text" => FontSizeClassicHeight90,
        "letterSpacing" if kind == "text" => LetterSpacingPx,
        "lineHeight" if kind == "text" => LineHeightMultiplier,
        "background.paddingX" if kind == "text" => TextBackgroundPaddingXAtSize15,
        "background.paddingY" if kind == "text" => TextBackgroundPaddingYAtSize15,
        "background.offsetX" if kind == "text" => TextBackgroundOffsetXPx,
        "background.offsetY" if kind == "text" => TextBackgroundOffsetYPx,
        "background.cornerRadius" if kind == "text" => TextBackgroundCornerRadiusPercent,
        _ => return None,
    };
    Some(ScalarBindingV4::Property { parameter: p })
}
fn handle(r: &mut Review, v: Option<&Value>, p: &str) -> Option<CurveHandleV4> {
    let v = v?;
    let o = r.object(v, p)?;
    r.keys(o, p, &["dt", "dv"]);
    if !o.contains_key("dv") {
        r.block(p, "INVALID_HANDLE", "Handle value offset required");
    }
    Some(CurveHandleV4 {
        dt: r.ticks(o, "dt", p, None),
        dv: r.num(o, "dv", p, 0.),
    })
}
// Source enums are Classic strings. ScalarSegmentV4 uses a tagged object on
// its wire, so deserializing the source as the target enum changes semantics.
fn source_enum<T: Clone>(
    r: &mut Review,
    o: &Map<String, Value>,
    key: &str,
    p: &str,
    values: &[(&str, T)],
    default: T,
) -> T {
    match o.get(key) {
        None => default,
        Some(Value::String(value)) => values
            .iter()
            .find(|(name, _)| *name == value)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| {
                r.block(
                    &format!("{p}.{key}"),
                    "UNSUPPORTED_VALUE",
                    "Unknown Classic curve enum value",
                );
                default
            }),
        Some(_) => {
            r.block(
                &format!("{p}.{key}"),
                "UNSUPPORTED_VALUE",
                "Expected Classic curve enum string",
            );
            default
        }
    }
}
fn segment(r: &mut Review, o: &Map<String, Value>, p: &str) -> ScalarSegmentV4 {
    source_enum(
        r,
        o,
        "segmentToNext",
        p,
        &[
            ("step", ScalarSegmentV4::Step),
            ("linear", ScalarSegmentV4::Linear),
            ("bezier", ScalarSegmentV4::Bezier),
        ],
        ScalarSegmentV4::Linear,
    )
}
fn tangent(r: &mut Review, o: &Map<String, Value>, p: &str) -> TangentModeV4 {
    source_enum(
        r,
        o,
        "tangentMode",
        p,
        &[
            ("auto", TangentModeV4::Auto),
            ("aligned", TangentModeV4::Aligned),
            ("broken", TangentModeV4::Broken),
            ("flat", TangentModeV4::Flat),
        ],
        TangentModeV4::Auto,
    )
}
fn extrapolate(r: &mut Review, o: &Map<String, Value>, key: &str, p: &str) -> ExtrapolationModeV4 {
    source_enum(
        r,
        o,
        key,
        p,
        &[
            ("hold", ExtrapolationModeV4::Hold),
            ("linear", ExtrapolationModeV4::Linear),
        ],
        ExtrapolationModeV4::Hold,
    )
}
fn channel(
    r: &mut Review,
    v: &Value,
    p: &str,
    id: &str,
    binding: ScalarBindingV4,
) -> Option<AnimationChannelV4> {
    let o = r.object(v, p)?;
    r.keys(o, p, &["keys", "extrapolation"]);
    let Some(keys) = v["keys"].as_array() else {
        r.block(
            &format!("{p}.keys"),
            "INVALID_CHANNEL",
            "Expected scalar keys",
        );
        return None;
    };
    let mut result = vec![];
    for (i, k) in keys.iter().enumerate() {
        let kp = format!("{p}.keys[{i}]");
        let Some(ko) = r.object(k, &kp) else { continue };
        r.keys(
            ko,
            &kp,
            &[
                "id",
                "time",
                "value",
                "leftHandle",
                "rightHandle",
                "segmentToNext",
                "tangentMode",
            ],
        );
        if !ko.get("value").is_some_and(Value::is_number) {
            r.block(
                &format!("{kp}.value"),
                "UNSUPPORTED_ANIMATION",
                "Only typed numeric curve values have a direct mapping",
            );
            continue;
        }
        if !ko.contains_key("segmentToNext") || !ko.contains_key("tangentMode") {
            r.block(
                &kp,
                "INVALID_CHANNEL",
                "v31 scalar keys require segment and tangent semantics",
            );
        }
        result.push(ScalarKeyV4 {
            id: r.string(ko, "id", &kp, None),
            segment_id: None,
            time: r.ticks(ko, "time", &kp, None),
            value: r.num(ko, "value", &kp, 0.),
            left_handle: handle(r, ko.get("leftHandle"), &format!("{kp}.leftHandle")),
            right_handle: handle(r, ko.get("rightHandle"), &format!("{kp}.rightHandle")),
            segment_to_next: segment(r, ko, &kp),
            tangent_mode: tangent(r, ko, &kp),
        });
    }
    let mut extrapolation = ExtrapolationV4 {
        before: ExtrapolationModeV4::Hold,
        after: ExtrapolationModeV4::Hold,
    };
    if let Some(v) = o.get("extrapolation") {
        if let Some(e) = r.object(v, &format!("{p}.extrapolation")) {
            r.keys(e, &format!("{p}.extrapolation"), &["before", "after"]);
            extrapolation = ExtrapolationV4 {
                before: extrapolate(r, e, "before", &format!("{p}.extrapolation")),
                after: extrapolate(r, e, "after", &format!("{p}.extrapolation")),
            };
        }
    }
    Some(AnimationChannelV4::Scalar {
        id: crate::content_id("classic_channel", format!("{id}:{p}").as_bytes()),
        target: ChannelTargetV4::Element {
            element_id: id.into(),
        },
        binding,
        keys: result,
        extrapolation,
    })
}
pub(super) fn parse(
    r: &mut Review,
    v: Option<&Value>,
    p: &str,
    id: &str,
    kind: &str,
) -> Vec<AnimationChannelV4> {
    let mut out = vec![];
    let Some(v) = v else { return out };
    let Some(o) = r.object(v, p) else { return out };
    for (path, value) in o {
        let cp = format!("{p}.{path}");
        if let Some(binding) = binding(path, kind) {
            if let Some(c) = channel(r, value, &cp, id, binding) {
                out.push(c);
            }
        } else if kind == "text" && (path == "color" || path == "background.color") {
            let Some(components) = r.object(value, &cp) else {
                continue;
            };
            r.keys(components, &cp, &["r", "g", "b", "a"]);
            for (k, component) in [
                ("r", ColorComponentV4::Red),
                ("g", ColorComponentV4::Green),
                ("b", ColorComponentV4::Blue),
                ("a", ColorComponentV4::Alpha),
            ] {
                if let Some(v) = components.get(k) {
                    let binding = ScalarBindingV4::ColorComponent {
                        parameter: if path == "color" {
                            ColorParameterV4::TextColor
                        } else {
                            ColorParameterV4::TextBackgroundColor
                        },
                        component,
                    };
                    if let Some(c) = channel(r, v, &format!("{cp}.{k}"), id, binding) {
                        out.push(c);
                    }
                } else {
                    r.block(
                        &format!("{cp}.{k}"),
                        "INVALID_COLOR_CHANNEL",
                        "Classic color channel requires all four components",
                    );
                }
            }
        } else {
            r.block(
                &cp,
                "UNSUPPORTED_ANIMATION",
                "Animation path has no verified typed mapping",
            );
        }
    }
    out
}
