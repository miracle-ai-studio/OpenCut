//! Preserve actual Classic curve controls and local clocks, never sample curves.
use super::*;
fn binding(path: &str, kind: &str, graphic: Option<graphics::Definition>) -> Option<ScalarBinding> {
    use ScalarParameter::*;
    let p = match path {
        "transform.positionX" if kind != "audio" && kind != "effect" => PositionXPx,
        "transform.positionY" if kind != "audio" && kind != "effect" => PositionYPx,
        "transform.scaleX" if kind != "audio" && kind != "effect" => ScaleX,
        "transform.scaleY" if kind != "audio" && kind != "effect" => ScaleY,
        "transform.rotate" if kind != "audio" && kind != "effect" => RotationDegrees,
        "opacity" if kind != "audio" && kind != "effect" => Opacity,
        "volume" if kind == "audio" || kind == "video" => GainDecibels,
        "fontSize" if kind == "text" => FontSizeClassicHeight90,
        "letterSpacing" if kind == "text" => LetterSpacingPx,
        "lineHeight" if kind == "text" => LineHeightMultiplier,
        "background.paddingX" if kind == "text" => TextBackgroundPaddingXAtSize15,
        "background.paddingY" if kind == "text" => TextBackgroundPaddingYAtSize15,
        "background.offsetX" if kind == "text" => TextBackgroundOffsetXPx,
        "background.offsetY" if kind == "text" => TextBackgroundOffsetYPx,
        "background.cornerRadius" if kind == "text" => TextBackgroundCornerRadiusPercent,
        "params.strokeWidth" if graphic.is_some() => GraphicStrokeWidthPx,
        "params.cornerRadius"
            if matches!(
                graphic,
                Some(graphics::Definition::Rectangle | graphics::Definition::Polygon)
            ) =>
        {
            GraphicCornerRadiusPercent
        }
        "params.sides" if matches!(graphic, Some(graphics::Definition::Polygon)) => GraphicSides,
        "params.points" if matches!(graphic, Some(graphics::Definition::Star)) => GraphicPoints,
        "params.depth" if matches!(graphic, Some(graphics::Definition::Star)) => {
            GraphicDepthPercent
        }
        _ => return None,
    };
    Some(ScalarBinding::Property { parameter: p })
}
fn color_parameter(path: &str, kind: &str) -> Option<ColorParameter> {
    match (kind, path) {
        ("text", "color") => Some(ColorParameter::TextColor),
        ("text", "background.color") => Some(ColorParameter::TextBackgroundColor),
        ("graphic", "params.fill") => Some(ColorParameter::GraphicFill),
        ("graphic", "params.stroke") => Some(ColorParameter::GraphicStroke),
        _ => None,
    }
}
fn handle(r: &mut Review, v: Option<&Value>, p: &str) -> Option<CurveHandle> {
    let v = v?;
    let o = r.object(v, p)?;
    r.keys(o, p, &["dt", "dv"]);
    if !o.contains_key("dv") {
        r.block(p, "INVALID_HANDLE", "Handle value offset required");
    }
    Some(CurveHandle {
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
fn segment(r: &mut Review, o: &Map<String, Value>, p: &str) -> ScalarSegment {
    source_enum(
        r,
        o,
        "segmentToNext",
        p,
        &[
            ("step", ScalarSegment::Step),
            ("linear", ScalarSegment::Linear),
            ("bezier", ScalarSegment::Bezier),
        ],
        ScalarSegment::Linear,
    )
}
fn tangent(r: &mut Review, o: &Map<String, Value>, p: &str) -> TangentMode {
    source_enum(
        r,
        o,
        "tangentMode",
        p,
        &[
            ("auto", TangentMode::Auto),
            ("aligned", TangentMode::Aligned),
            ("broken", TangentMode::Broken),
            ("flat", TangentMode::Flat),
        ],
        TangentMode::Auto,
    )
}
fn extrapolate(r: &mut Review, o: &Map<String, Value>, key: &str, p: &str) -> ExtrapolationMode {
    source_enum(
        r,
        o,
        key,
        p,
        &[
            ("hold", ExtrapolationMode::Hold),
            ("linear", ExtrapolationMode::Linear),
        ],
        ExtrapolationMode::Hold,
    )
}
fn channel(
    r: &mut Review,
    v: &Value,
    p: &str,
    id: &str,
    binding: ScalarBinding,
) -> Option<AnimationChannel> {
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
        result.push(ScalarKey {
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
    let mut extrapolation = Extrapolation {
        before: ExtrapolationMode::Hold,
        after: ExtrapolationMode::Hold,
    };
    if let Some(v) = o.get("extrapolation") {
        if let Some(e) = r.object(v, &format!("{p}.extrapolation")) {
            r.keys(e, &format!("{p}.extrapolation"), &["before", "after"]);
            extrapolation = Extrapolation {
                before: extrapolate(r, e, "before", &format!("{p}.extrapolation")),
                after: extrapolate(r, e, "after", &format!("{p}.extrapolation")),
            };
        }
    }
    Some(AnimationChannel::Scalar {
        id: crate::content_id("classic_channel", format!("{id}:{p}").as_bytes()),
        target: ChannelTarget::Element {
            element_id: id.into(),
        },
        binding,
        keys: result,
        extrapolation,
    })
}
// Classic discrete channels contain only id/time/value keys. Their implicit
// hold interpolation maps directly; scalar controls or extrapolation reject.
fn stroke_align_channel(
    r: &mut Review,
    v: &Value,
    p: &str,
    id: &str,
    initial: StrokeAlign,
) -> Option<AnimationChannel> {
    let o = r.object(v, p)?;
    r.keys(o, p, &["keys"]);
    let Some(keys) = v["keys"].as_array() else {
        r.block(
            &format!("{p}.keys"),
            "INVALID_CHANNEL",
            "Expected discrete keys",
        );
        return None;
    };
    let mut result = vec![];
    for (index, key) in keys.iter().enumerate() {
        let kp = format!("{p}.keys[{index}]");
        let Some(ko) = r.object(key, &kp) else {
            continue;
        };
        r.keys(ko, &kp, &["id", "time", "value"]);
        if !ko.contains_key("value") {
            r.block(
                &format!("{kp}.value"),
                "INVALID_CHANNEL",
                "Stroke alignment value required",
            );
        }
        result.push(DiscreteKey {
            id: r.string(ko, "id", &kp, None),
            time: r.ticks(ko, "time", &kp, None),
            value: DiscreteValue::StrokeAlign {
                value: r.enumeration(ko, "value", &kp, StrokeAlign::Center),
            },
        });
    }
    // Classic keeps the authored parameter until its first discrete key. V4
    // holds the first key before its time, so retain that interval explicitly.
    // Do not move, resample or replace any of the original keys.
    if result
        .first()
        .is_some_and(|key| key.time.ticks.parse::<i64>().is_ok_and(|t| t > 0))
    {
        result.insert(
            0,
            DiscreteKey {
                id: crate::content_id("classic_initial_key", format!("{id}:{p}").as_bytes()),
                time: t(0),
                value: DiscreteValue::StrokeAlign { value: initial },
            },
        );
    }
    Some(AnimationChannel::Discrete {
        id: crate::content_id("classic_channel", format!("{id}:{p}").as_bytes()),
        target: ChannelTarget::Element {
            element_id: id.into(),
        },
        parameter: DiscreteParameter::GraphicStrokeAlign,
        keys: result,
    })
}
pub(super) fn parse(
    r: &mut Review,
    v: Option<&Value>,
    p: &str,
    id: &str,
    kind: &str,
    graphic: Option<graphics::Definition>,
    effects: &[SceneEffect],
    initial_stroke_align: StrokeAlign,
) -> Vec<AnimationChannel> {
    let mut out = vec![];
    let Some(v) = v else { return out };
    let Some(o) = r.object(v, p) else { return out };
    for (path, value) in o {
        let cp = format!("{p}.{path}");
        // Exact source builder: effects.${id}.params.${key}. The Classic
        // standalone EffectLayerNode has no animation input, and masks have no
        // animation-path builder, so do not assign semantics to either here.
        if let Some((effect_id, param)) = path
            .strip_prefix("effects.")
            .and_then(|p| p.split_once(".params."))
        {
            if let Some(effect) = effects.iter().find(|effect| effect.id == effect_id) {
                if param == "intensity" && matches!(effect.effect, EffectKind::ClassicBlur { .. }) {
                    if let Some(mut c) = channel(
                        r,
                        value,
                        &cp,
                        id,
                        ScalarBinding::Property {
                            parameter: ScalarParameter::ClassicBlurIntensity,
                        },
                    ) {
                        if let AnimationChannel::Scalar { target, .. } = &mut c {
                            *target = ChannelTarget::Effect {
                                element_id: id.into(),
                                effect_id: effect_id.into(),
                            };
                        }
                        out.push(c);
                    }
                    continue;
                }
            }
            r.block(
                &cp,
                "UNSUPPORTED_ANIMATION",
                "Effect path requires an existing Classic blur and intensity parameter",
            );
        } else if let Some(binding) = binding(path, kind, graphic) {
            if let Some(c) = channel(r, value, &cp, id, binding) {
                out.push(c);
            }
        } else if let Some(parameter) = color_parameter(path, kind) {
            let Some(components) = r.object(value, &cp) else {
                continue;
            };
            r.keys(components, &cp, &["r", "g", "b", "a"]);
            for (k, component) in [
                ("r", ColorComponent::Red),
                ("g", ColorComponent::Green),
                ("b", ColorComponent::Blue),
                ("a", ColorComponent::Alpha),
            ] {
                if let Some(v) = components.get(k) {
                    let binding = ScalarBinding::ColorComponent {
                        parameter,
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
        } else if graphic.is_some() && path == "params.strokeAlign" {
            if let Some(channel) = stroke_align_channel(r, value, &cp, id, initial_stroke_align) {
                out.push(channel);
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
