//! Generated graphics partition over the original sampled graph; no resource projection.
use super::*;
use opencut_contracts::{graphic_frame_v1::*, visual_frame_v4::VisualQuadV4};
pub const CLASSIC_GRAPHIC_FRAME_PROFILE_V1: &str = "classic-v31-gpu-normal-graphic-frame-v1";
#[derive(Clone, Debug)]
pub struct PreparedGraphicFrameV1 {
    source_graph: CompositionV4,
    execution: PreparedTextFrameV3,
    graphics: Vec<GraphicLayerPlanV1>,
    order: Vec<String>,
}
impl PreparedGraphicFrameV1 {
    pub fn source_graph(&self) -> &CompositionV4 {
        &self.source_graph
    }
    pub fn execution(&self) -> &PreparedTextFrameV3 {
        &self.execution
    }
    pub fn graphics(&self) -> &[GraphicLayerPlanV1] {
        &self.graphics
    }
    pub fn order(&self) -> &[String] {
        &self.order
    }
}
fn finite(value: f64) -> Result<f32, CompositionError> {
    let cast = value as f32;
    if !value.is_finite() || !cast.is_finite() {
        return Err(invalid("Graphic geometry/quad is not finite f32"));
    }
    Ok(cast)
}
fn color(value: &ColorV4) -> Result<[f64; 4], CompositionError> {
    let values = [value.red, value.green, value.blue, value.alpha];
    if value.encoding != ColorEncodingV4::Srgb
        || values
            .iter()
            .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
    {
        return Err(unsupported("Graphic raster requires bounded sRGB color"));
    }
    Ok(values)
}
pub fn graphic_raster_receipt_v1(
    layer: &GraphicLayerPlanV1,
) -> Result<GraphicRasterReceiptV1, CompositionError> {
    let g = &layer.graphic;
    let width = g.stroke.width_px;
    if g.source_width == 0
        || g.source_height == 0
        || !width.is_finite()
        || !(width * 2.0).is_finite()
        || width < 0.0
    {
        return Err(invalid("Graphic raster source/stroke is invalid"));
    }
    let padding = if width == 0.0 || g.stroke.align == StrokeAlignV4::Inside {
        0.0
    } else {
        (width
            * if g.stroke.align == StrokeAlignV4::Center {
                5.0
            } else {
                10.0
            })
        .ceil()
    };
    if !padding.is_finite() || padding > f64::from(u32::MAX) {
        return Err(invalid("Graphic raster padding overflows"));
    }
    let padding = padding as u32;
    let twice = padding
        .checked_mul(2)
        .ok_or_else(|| invalid("Graphic raster padding overflows"))?;
    let receipt = GraphicRasterReceiptV1 {
        element_id: layer.element_id.clone(),
        width: g
            .source_width
            .checked_add(twice)
            .ok_or_else(|| invalid("Graphic raster width overflows"))?,
        height: g
            .source_height
            .checked_add(twice)
            .ok_or_else(|| invalid("Graphic raster height overflows"))?,
        origin_x: padding,
        origin_y: padding,
    };
    if receipt.width > 16384 || receipt.height > 16384 {
        return Err(invalid("Graphic raster dimensions exceed 16384"));
    }
    Ok(receipt)
}
pub fn graphic_raster_receipts_v1(
    prepared: &PreparedGraphicFrameV1,
) -> Result<Vec<GraphicRasterReceiptV1>, CompositionError> {
    prepared
        .graphics
        .iter()
        .map(graphic_raster_receipt_v1)
        .collect()
}
pub fn validate_graphic_receipts_v1(
    prepared: &PreparedGraphicFrameV1,
    receipts: &[GraphicRasterReceiptV1],
) -> Result<(), CompositionError> {
    if graphic_raster_receipts_v1(prepared)? != receipts {
        return Err(invalid(
            "Graphic receipts differ from fixed order/source/padding",
        ));
    }
    Ok(())
}
pub fn prepare_graphic_frame_v1(
    graph: &CompositionV4,
) -> Result<PreparedGraphicFrameV1, CompositionError> {
    reject_legacy_synthesis_fonts(&graph.fonts)?;
    let mut execution = graph.clone();
    let mut graphics = Vec::new();
    let mut order = Vec::new();
    let mut ids = HashSet::new();
    for layer in &graph.layers {
        if layer.element_id.is_empty() || !ids.insert(&layer.element_id) {
            return Err(invalid(
                "Graphic frame layer identity is empty or duplicate",
            ));
        }
        order.push(layer.element_id.clone());
        let SampledContentV4::Graphic { graphic } = &layer.content else {
            continue;
        };
        let style = layer
            .visual
            .as_ref()
            .ok_or_else(|| unsupported("Graphic visual style missing"))?;
        if style.profile != VisualProfileV4::ClassicCanvasV31
            || style.blend_mode != BlendModeV4::Normal
            || style.effects.iter().any(|e| e.enabled)
            || style.masks.iter().any(|m| m.enabled)
        {
            return Err(unsupported(
                "Graphic frame supports only normal Classic graphics without effective effects/masks",
            ));
        }
        let geometry = match graphic.geometry {
            GraphicGeometryV4::Rectangle {
                corner_radius_percent,
            } if corner_radius_percent.is_finite()
                && (0.0..=100.0).contains(&corner_radius_percent) =>
            {
                GraphicGeometryPlanV1::Rectangle {
                    corner_radius_percent,
                }
            }
            GraphicGeometryV4::Ellipse => GraphicGeometryPlanV1::Ellipse,
            GraphicGeometryV4::Polygon {
                sides,
                corner_radius_percent,
            } if sides.is_finite()
                && sides >= 3.0
                && corner_radius_percent.is_finite()
                && (0.0..=100.0).contains(&corner_radius_percent) =>
            {
                GraphicGeometryPlanV1::Polygon {
                    sides,
                    corner_radius_percent,
                }
            }
            GraphicGeometryV4::Star {
                points,
                depth_percent,
            } if points.is_finite()
                && points >= 2.0
                && depth_percent.is_finite()
                && (0.0..=100.0).contains(&depth_percent) =>
            {
                GraphicGeometryPlanV1::Star {
                    points,
                    depth_percent,
                }
            }
            GraphicGeometryV4::LegacyRectangle { .. } | GraphicGeometryV4::LegacyEllipse { .. } => {
                return Err(unsupported(
                    "Legacy graphic geometry requires its own execution profile",
                ));
            }
            _ => return Err(invalid("Graphic geometry has invalid parametric values")),
        };
        let mut plan = GraphicLayerPlanV1 {
            element_id: layer.element_id.clone(),
            graphic: GraphicRasterPlanV1 {
                geometry,
                source_width: graphic.source_size.width,
                source_height: graphic.source_size.height,
                fill: color(&graphic.fill)?,
                stroke: GraphicStrokePlanV1 {
                    color: color(&graphic.stroke.color)?,
                    width_px: graphic.stroke.width_px,
                    align: graphic.stroke.align,
                },
            },
            quad: None,
        };
        let receipt = graphic_raster_receipt_v1(&plan)?;
        let t = style.transform;
        let contain = (f64::from(graph.format.canvas_size.width)
            / f64::from(graphic.source_size.width))
        .min(f64::from(graph.format.canvas_size.height) / f64::from(graphic.source_size.height));
        let w = f64::from(receipt.width) * contain * t.scale_x;
        let h = f64::from(receipt.height) * contain * t.scale_y;
        let quad = VisualQuadV4 {
            center_x: finite(f64::from(graph.format.canvas_size.width) / 2.0 + t.position_x_px)?,
            center_y: finite(f64::from(graph.format.canvas_size.height) / 2.0 + t.position_y_px)?,
            width: finite(w.abs())?,
            height: finite(h.abs())?,
            rotation_radians: finite(t.rotation_degrees)?.to_radians(),
            flip_x: w < 0.0,
            flip_y: h < 0.0,
            opacity: finite(style.effective_opacity)?,
        };
        if !quad.rotation_radians.is_finite() {
            return Err(invalid("Graphic rotation overflows"));
        }
        if quad.width != 0.0 && quad.height != 0.0 {
            plan.quad = Some(quad)
        }
        graphics.push(plan);
    }
    if order.len() > 60 {
        return Err(unsupported("Graphic frame exceeds 60 layers"));
    }
    execution
        .layers
        .retain(|layer| !matches!(layer.content, SampledContentV4::Graphic { .. }));
    Ok(PreparedGraphicFrameV1 {
        source_graph: graph.clone(),
        execution: prepare_text_frame_v3(&execution)?,
        graphics,
        order,
    })
}
