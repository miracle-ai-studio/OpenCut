//! Classic masks/types.ts, builtin/definitions and freeform/path.ts.
//! Persisted parameters are complete. Creation defaults depend on display bounds
//! and cannot be inferred when importing a partially serialized document.
use super::*;
const BASE: &[&str] = &[
    "feather",
    "inverted",
    "strokeColor",
    "strokeWidth",
    "strokeAlign",
    "centerX",
    "centerY",
    "rotation",
];
const BOX: &[&str] = &["width", "height", "scale"];
const TEXT: &[&str] = &[
    "content",
    "fontSize",
    "fontFamily",
    "fontWeight",
    "fontStyle",
    "textDecoration",
    "letterSpacing",
    "lineHeight",
    "scale",
];
const FREEFORM: &[&str] = &["path", "closed", "scale"];
fn complete(r: &mut Review, o: &Map<String, Value>, p: &str, keys: &[&str]) {
    r.keys(o, p, keys);
    for key in keys {
        if !o.contains_key(*key) {
            r.block(&format!("{p}.{key}"), "MISSING_MASK_PARAMETER", "Persisted Classic masks require complete parameters; creation defaults cannot be inferred");
        }
    }
}
pub(super) fn parse(
    r: &mut Review,
    value: Option<&Value>,
    p: &str,
    kind: &str,
    res: &ClassicResources,
) -> Vec<Mask> {
    let Some(value) = value else { return vec![] };
    let Some(values) = value.as_array() else {
        r.block(p, "INVALID_DEPENDENCY", "Expected masks array");
        return vec![];
    };
    if values.is_empty() {
        return vec![];
    }
    if !matches!(kind, "video" | "image" | "graphic") {
        r.block(
            p,
            "UNSUPPORTED_MASK_TARGET",
            "Classic masks require video, image or graphic content",
        );
        return vec![];
    }
    if values.len() > 1 {
        r.loss(p, "CLASSIC_INACTIVE_MASKS_RETAINED", "Classic renders only the first mask. Later masks retain all parameters and original order as disabled masks; explicitly enable them to apply a new V4 stack.");
    }
    values
        .iter()
        .enumerate()
        .filter_map(|(i, v)| {
            let mut mask = parse_one(r, v, &format!("{p}[{i}]"), res)?;
            mask.enabled = i == 0;
            Some(mask)
        })
        .collect()
}
fn parse_one(r: &mut Review, value: &Value, p: &str, res: &ClassicResources) -> Option<Mask> {
    let o = r.object(value, p)?;
    r.keys(o, p, &["id", "type", "params"]);
    let id = r.string(o, "id", p, None);
    let kind = r.string(o, "type", p, None);
    let extra = match kind.as_str() {
        "rectangle" | "ellipse" | "cinematic-bars" | "heart" | "diamond" | "star" => BOX,
        "text" => TEXT,
        "freeform" => FREEFORM,
        "split" => &[],
        _ => {
            r.block(
                &format!("{p}.type"),
                "UNSUPPORTED_MASK",
                "Unknown Classic mask type",
            );
            return None;
        }
    };
    let pp = format!("{p}.params");
    let params = r.object(&value["params"], &pp)?;
    let keys = BASE.iter().chain(extra.iter()).copied().collect::<Vec<_>>();
    complete(r, params, &pp, &keys);
    let x = r.num(params, "centerX", &pp, 0.);
    let y = r.num(params, "centerY", &pp, 0.);
    let rotation = r.num(params, "rotation", &pp, 0.);
    let shape = match kind.as_str() {
        "split" => LayerMaskShape::Split {
            center_x: x,
            center_y: y,
            rotation_degrees: rotation,
        },
        "text" => {
            let weight = r.enumeration(params, "fontWeight", &pp, FontWeight::Normal);
            let style = r.enumeration(params, "fontStyle", &pp, FontStyle::Normal);
            let family = r.string(params, "fontFamily", &pp, None);
            let font = elements::font_binding(r, family, weight, style, &pp, res);
            let decoration =
                if params.get("textDecoration").and_then(Value::as_str) == Some("line-through") {
                    TextDecoration::LineThrough
                } else {
                    r.enumeration(params, "textDecoration", &pp, TextDecoration::None)
                };
            LayerMaskShape::Text {
                mask: TextMask {
                    content: r.string(params, "content", &pp, None),
                    font,
                    font_weight: weight,
                    font_style: style,
                    font_size: FontSize::ClassicHeight90 {
                        value: r.num(params, "fontSize", &pp, 15.),
                    },
                    decoration,
                    letter_spacing_px: r.num(params, "letterSpacing", &pp, 0.),
                    line_height_multiplier: r.num(params, "lineHeight", &pp, 1.2),
                    center_x: x,
                    center_y: y,
                    rotation_degrees: rotation,
                    scale: r.num(params, "scale", &pp, 1.),
                },
            }
        }
        "freeform" => {
            let mut points = vec![];
            if let Some(list) = params.get("path").and_then(Value::as_array) {
                for (i, v) in list.iter().enumerate() {
                    let point_p = format!("{pp}.path[{i}]");
                    let Some(o) = r.object(v, &point_p) else {
                        continue;
                    };
                    complete(
                        r,
                        o,
                        &point_p,
                        &["id", "x", "y", "inX", "inY", "outX", "outY"],
                    );
                    let x = r.num(o, "x", &point_p, 0.);
                    let y = r.num(o, "y", &point_p, 0.);
                    // Classic handles are relative offsets. V4 uses absolute
                    // element-local positions; do not rescale by the canvas.
                    points.push(FreeformPoint {
                        id: r.string(o, "id", &point_p, None),
                        x,
                        y,
                        in_x: x + r.num(o, "inX", &point_p, 0.),
                        in_y: y + r.num(o, "inY", &point_p, 0.),
                        out_x: x + r.num(o, "outX", &point_p, 0.),
                        out_y: y + r.num(o, "outY", &point_p, 0.),
                    });
                }
            } else {
                r.block(
                    &format!("{pp}.path"),
                    "INVALID_MASK_PATH",
                    "Expected complete Classic freeform point array",
                );
            }
            LayerMaskShape::Freeform {
                path: FreeformMask {
                    points,
                    closed: r.boolean(params, "closed", &pp, false),
                    center_x: x,
                    center_y: y,
                    rotation_degrees: rotation,
                    scale: r.num(params, "scale", &pp, 1.),
                },
            }
        }
        _ => {
            let geometry = BoxMaskGeometry {
                center_x: x,
                center_y: y,
                width: r.num(params, "width", &pp, 0.6),
                height: r.num(params, "height", &pp, 0.6),
                rotation_degrees: rotation,
                scale: r.num(params, "scale", &pp, 1.),
            };
            match kind.as_str() {
                "rectangle" => LayerMaskShape::Rectangle { geometry },
                "ellipse" => LayerMaskShape::Ellipse { geometry },
                "cinematic-bars" => LayerMaskShape::CinematicBars { geometry },
                "heart" => LayerMaskShape::Heart { geometry },
                "diamond" => LayerMaskShape::Diamond { geometry },
                _ => LayerMaskShape::Star { geometry },
            }
        }
    };
    let mask = Mask {
        id,
        enabled: true,
        shape,
        inverted: r.boolean(params, "inverted", &pp, false),
        feather: MaskFeather::ClassicPixels {
            value: r.num(params, "feather", &pp, 0.),
        },
        stroke: Stroke {
            color: parse_color(r, params, "strokeColor", &pp, "#ffffff"),
            width_px: r.num(params, "strokeWidth", &pp, 0.),
            align: r.enumeration(params, "strokeAlign", &pp, StrokeAlign::Center),
        },
    };
    r.report.mapped.push(p.into());
    Some(mask)
}
