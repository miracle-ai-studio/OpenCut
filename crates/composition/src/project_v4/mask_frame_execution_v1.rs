//! Classic box masks wrap the frozen blend execution. Mask geometry uses the
//! logical element extent, never a graphic raster's transparent stroke padding.
use super::*;
use opencut_contracts::{
    blend_frame_v1::LayerBlendV1,
    graphic_frame_v1::BoundGraphicFrameV1,
    mask_frame_v1::{BoundLayerMaskV1, LayerMaskV1, MaskQuadV1},
};

pub const CLASSIC_MASK_FRAME_PROFILE_V1: &str = "classic-v31-gpu-mask-frame-v1";

#[derive(Clone, Debug)]
pub struct PreparedMaskFrameV1 {
    source_graph: CompositionV4,
    blend: PreparedBlendFrameV1,
    masks: Vec<LayerMaskV1>,
}
impl PreparedMaskFrameV1 {
    pub fn source_graph(&self) -> &CompositionV4 {
        &self.source_graph
    }
    pub fn blend(&self) -> &PreparedBlendFrameV1 {
        &self.blend
    }
    pub fn execution(&self) -> &PreparedGraphicFrameV1 {
        self.blend.execution()
    }
    pub fn blends(&self) -> &[LayerBlendV1] {
        self.blend.blends()
    }
    pub fn masks(&self) -> &[LayerMaskV1] {
        &self.masks
    }
}

fn validate_mask(mask: &MaskV4) -> Result<(), CompositionError> {
    let geometry = match &mask.shape {
        MaskShapeV4::Rectangle { geometry } | MaskShapeV4::Ellipse { geometry } => geometry,
        _ => {
            return Err(unsupported(
                "Mask frame supports only Classic rectangle/ellipse masks",
            ));
        }
    };
    let MaskFeatherV4::ClassicPixels { value: feather } = mask.feather else {
        return Err(unsupported(
            "Mask frame requires Classic pixel feather units",
        ));
    };
    if mask.inverted || mask.stroke.width_px != 0.0 {
        return Err(unsupported(
            "Mask frame excludes inverted masks and mask strokes",
        ));
    }
    if mask.id.is_empty()
        || ![
            geometry.center_x,
            geometry.center_y,
            geometry.width,
            geometry.height,
            geometry.rotation_degrees,
            geometry.scale,
            feather,
        ]
        .iter()
        .all(|v| v.is_finite())
        || geometry.width <= 0.0
        || geometry.height <= 0.0
        || geometry.scale < 0.0
        || feather < 0.0
        || !(feather as f32).is_finite()
        || ((feather as f32) > 0.0 && (feather as f32) / 2.0 == 0.0)
        || mask.stroke.color.encoding != ColorEncodingV4::Srgb
        || [
            mask.stroke.color.red,
            mask.stroke.color.green,
            mask.stroke.color.blue,
            mask.stroke.color.alpha,
        ]
        .iter()
        .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
    {
        return Err(invalid(
            "Mask frame contains invalid mask geometry, feather or color",
        ));
    }
    Ok(())
}

pub fn prepare_mask_frame_v1(
    graph: &CompositionV4,
) -> Result<PreparedMaskFrameV1, CompositionError> {
    reject_legacy_synthesis_fonts(&graph.fonts)?;
    let mut delegated = graph.clone();
    let mut masks = Vec::with_capacity(graph.layers.len());
    for layer in &mut delegated.layers {
        let visual = layer
            .visual
            .as_mut()
            .ok_or_else(|| unsupported("Mask frame requires a visual style for every layer"))?;
        let mut enabled = visual.masks.iter().filter(|mask| mask.enabled);
        let mask = enabled.next();
        if enabled.next().is_some() {
            return Err(unsupported(
                "Mask frame supports at most one enabled mask per element",
            ));
        }
        if let Some(mask) = mask {
            if !matches!(
                layer.content,
                SampledContentV4::Video { .. }
                    | SampledContentV4::Image { .. }
                    | SampledContentV4::Graphic { .. }
            ) {
                return Err(unsupported(
                    "Only video, image and graphic elements support Classic box masks",
                ));
            }
            validate_mask(mask)?;
        }
        masks.push(LayerMaskV1 {
            element_id: layer.element_id.clone(),
            mask: mask.cloned(),
        });
        // The original graph above remains the authority. This copy only
        // delegates decoding/rasterization/blending to the frozen profiles.
        visual.masks.clear();
    }
    let blend = prepare_blend_frame_v1(&delegated)?;
    if !blend
        .execution()
        .order()
        .iter()
        .eq(masks.iter().map(|layer| &layer.element_id))
    {
        return Err(invalid(
            "Mask table differs from the complete execution order",
        ));
    }
    Ok(PreparedMaskFrameV1 {
        source_graph: graph.clone(),
        blend,
        masks,
    })
}

pub fn validate_mask_layers_v1(
    prepared: &PreparedMaskFrameV1,
    masks: &[LayerMaskV1],
) -> Result<(), CompositionError> {
    if prepared.masks != masks {
        return Err(invalid(
            "Mask facts differ from fixed layer identities, order or masks",
        ));
    }
    Ok(())
}

pub fn bind_mask_layers_v1(
    prepared: &PreparedMaskFrameV1,
    child: &BoundGraphicFrameV1,
) -> Result<Vec<BoundLayerMaskV1>, CompositionError> {
    let execution = prepared.execution();
    let text = execution.execution().execution();
    let fixed = text.identity();
    if child.identity.project_id != fixed.project_id
        || child.identity.revision != fixed.revision
        || child.identity.sequence_id != fixed.sequence_id
        || child.identity.time != fixed.time
        || child.identity.width != fixed.width
        || child.identity.height != fixed.height
        || child.identity.profile != CLASSIC_GRAPHIC_FRAME_PROFILE_V1
        || !execution
            .order()
            .iter()
            .eq(child.layers.iter().map(|layer| &layer.element_id))
    {
        return Err(invalid(
            "Delegated mask frame differs from fixed identity or complete order",
        ));
    }
    validate_graphic_receipts_v1(execution, &child.graphics)?;
    // Decoded dimensions/NoFrame are observations. Reuse the existing binder to
    // validate them and derive media geometry rather than accepting client quads.
    let media = bind_visual_sources_v4(text.media(), &child.decoded)?;
    let mut result = Vec::with_capacity(prepared.masks.len());
    for ((mask, layer), source) in prepared
        .masks
        .iter()
        .zip(&child.layers)
        .zip(&prepared.source_graph.layers)
    {
        let graphic = execution
            .graphics()
            .iter()
            .find(|graphic| graphic.element_id == layer.element_id);
        let size = if let Some(graphic) = graphic {
            if layer.asset_id != format!("graphic:{}", layer.element_id)
                || layer.quad != graphic.quad
            {
                return Err(invalid(
                    "Delegated graphic mask quad differs from fixed geometry",
                ));
            }
            Some((graphic.graphic.source_width, graphic.graphic.source_height))
        } else if let Some(expected) = media
            .layers
            .iter()
            .find(|item| item.element_id == layer.element_id)
        {
            if layer != expected {
                return Err(invalid(
                    "Delegated media mask quad differs from validated source geometry",
                ));
            }
            child.decoded.iter().find_map(|decoded| match decoded {
                DecodedVisualSourceV4::ReadyImage {
                    element_id,
                    width,
                    height,
                }
                | DecodedVisualSourceV4::ReadyVideo {
                    element_id,
                    width,
                    height,
                    ..
                } if element_id == &layer.element_id => Some((*width, *height)),
                _ => None,
            })
        } else {
            // Text/caption quads are validated by the existing text binder and
            // can never be mask targets in this execution profile.
            None
        };
        let quad = if mask.mask.is_some() && layer.quad.is_some() {
            match size {
                Some((width, height)) => {
                    logical_quad(&prepared.source_graph, source, width, height)?
                }
                None => None,
            }
        } else {
            None
        };
        result.push(BoundLayerMaskV1 {
            element_id: mask.element_id.clone(),
            mask: mask.mask.clone(),
            quad,
        });
    }
    Ok(result)
}

/// Shared Classic logical bounds for a sampled layer and positive upright source
/// dimensions. Callers must admit the layer's visual style and source dimensions
/// before calling; this pure geometry does not authorize or decode a resource.
pub fn logical_quad(
    graph: &CompositionV4,
    layer: &CompiledLayerV4,
    source_width: u32,
    source_height: u32,
) -> Result<Option<MaskQuadV1>, CompositionError> {
    let style = layer.visual.as_ref().expect("prepared mask visual");
    let transform = style.transform;
    let width = f64::from(graph.format.canvas_size.width);
    let height = f64::from(graph.format.canvas_size.height);
    let contain = (width / f64::from(source_width)).min(height / f64::from(source_height));
    let signed_width = f64::from(source_width) * contain * transform.scale_x;
    let signed_height = f64::from(source_height) * contain * transform.scale_y;
    // Classic draws the local mask canvas before converting the layer's GPU
    // uniforms to f32. Keep this f64 arithmetic and the original degree value:
    // e.g. round(10.4999999) differs from round(f32(10.4999999)).
    let quad = MaskQuadV1 {
        center_x: width / 2.0 + transform.position_x_px,
        center_y: height / 2.0 + transform.position_y_px,
        width: signed_width.abs(),
        height: signed_height.abs(),
        rotation_radians: transform.rotation_degrees * std::f64::consts::PI / 180.0,
        flip_x: signed_width < 0.0,
        flip_y: signed_height < 0.0,
        opacity: style.effective_opacity,
    };
    if [
        quad.center_x,
        quad.center_y,
        quad.width,
        quad.height,
        quad.rotation_radians,
        quad.opacity,
    ]
    .iter()
    .any(|value| !value.is_finite())
    {
        return Err(invalid("Logical mask geometry is not finite"));
    }
    Ok((quad.width != 0.0 && quad.height != 0.0).then_some(quad))
}

#[cfg(test)]
mod tests;
