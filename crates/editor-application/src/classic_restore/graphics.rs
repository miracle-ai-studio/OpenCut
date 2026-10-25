//! Classic cf5e79e graphics/definitions and graphics/types.ts. Preserve authored
//! parameters; the existing V4 sampler owns Classic clamping and count rounding.
use super::*;

#[derive(Clone, Copy)]
pub(super) enum Definition {
    Rectangle,
    Ellipse,
    Polygon,
    Star,
}
impl Definition {
    pub(super) fn parameters(self) -> &'static [&'static str] {
        match self {
            Self::Rectangle => &[
                "fill",
                "stroke",
                "strokeWidth",
                "strokeAlign",
                "cornerRadius",
            ],
            Self::Ellipse => &["fill", "stroke", "strokeWidth", "strokeAlign"],
            Self::Polygon => &[
                "fill",
                "stroke",
                "strokeWidth",
                "strokeAlign",
                "sides",
                "cornerRadius",
            ],
            Self::Star => &[
                "fill",
                "stroke",
                "strokeWidth",
                "strokeAlign",
                "points",
                "depth",
            ],
        }
    }
}
pub(super) fn definition(
    r: &mut Review,
    element: &Map<String, Value>,
    p: &str,
) -> Option<Definition> {
    match r.string(element, "definitionId", p, None).as_str() {
        "rectangle" => Some(Definition::Rectangle),
        "ellipse" => Some(Definition::Ellipse),
        "polygon" => Some(Definition::Polygon),
        "star" => Some(Definition::Star),
        _ => {
            r.block(
                &format!("{p}.definitionId"),
                "UNSUPPORTED_GRAPHIC",
                "Only the four fixed Classic graphic definitions have a direct mapping",
            );
            None
        }
    }
}
pub(super) fn parse(
    r: &mut Review,
    params: &Map<String, Value>,
    p: &str,
    definition: Definition,
) -> Graphic {
    let geometry = match definition {
        Definition::Rectangle => GraphicGeometry::Rectangle {
            corner_radius_percent: r.num(params, "cornerRadius", p, 0.),
        },
        Definition::Ellipse => GraphicGeometry::Ellipse,
        Definition::Polygon => GraphicGeometry::Polygon {
            sides: r.num(params, "sides", p, 5.),
            corner_radius_percent: r.num(params, "cornerRadius", p, 0.),
        },
        Definition::Star => GraphicGeometry::Star {
            points: r.num(params, "points", p, 5.),
            depth_percent: r.num(params, "depth", p, 45.),
        },
    };
    Graphic {
        geometry,
        source_size: CanvasSize {
            width: 512,
            height: 512,
        },
        fill: parse_color(r, params, "fill", p, "#ffffff"),
        stroke: Stroke {
            color: parse_color(r, params, "stroke", p, "#000000"),
            width_px: r.num(params, "strokeWidth", p, 0.),
            align: r.enumeration(params, "strokeAlign", p, StrokeAlign::Center),
        },
    }
}
