//! Fixed-source authoring bounds use the sampled IR and the renderer's shared
//! double-precision contain transform. No media/font I/O or static-scene fallback.
use opencut_composition::project_v4::{
    CompiledLayerV4, CompositionV4, SampledContentV4, logical_quad,
};
use opencut_contracts::{
    EditorError, element_transform_v2::SampledElementGeometryV2, project_v4::CanvasSizeV4,
};

fn unsupported(message: &str) -> EditorError {
    EditorError::new("UNSUPPORTED_VISUAL_FEATURE", message)
}

pub(super) fn sampled_geometry(
    graph: &CompositionV4,
    layer: &CompiledLayerV4,
) -> Result<Option<SampledElementGeometryV2>, EditorError> {
    let size = match &layer.content {
        SampledContentV4::Text { .. } => return Ok(None),
        SampledContentV4::Graphic { graphic } => graphic.source_size,
        SampledContentV4::Video { asset_id, .. }
        | SampledContentV4::Image { asset_id }
        | SampledContentV4::Sticker { asset_id, .. } => {
            // Sticker's authored intrinsic_size is not a decoded observation.
            // Like image/video, use only the fixed asset's observed dimensions.
            let resource = graph
                .resources
                .iter()
                .find(|resource| &resource.asset_id == asset_id)
                .ok_or_else(|| unsupported("Transform source resource is missing"))?;
            let size = resource.metadata.intrinsic_size.ok_or_else(|| {
                unsupported("Transform geometry requires observed media dimensions")
            })?;
            let rotation = resource
                .metadata
                .rotation_degrees
                .unwrap_or(0.0)
                .rem_euclid(360.0);
            if rotation == 0.0 || rotation == 180.0 {
                size
            } else if rotation == 90.0 || rotation == 270.0 {
                CanvasSizeV4 {
                    width: size.height,
                    height: size.width,
                }
            } else {
                return Err(unsupported(
                    "Transform geometry requires observed quarter-turn media orientation",
                ));
            }
        }
        _ => {
            return Err(unsupported(
                "Transform geometry requires a visual content element",
            ));
        }
    };
    if size.width == 0
        || size.height == 0
        || graph.format.canvas_size.width == 0
        || graph.format.canvas_size.height == 0
        || layer.visual.is_none()
    {
        return Err(unsupported(
            "Transform geometry requires positive source/canvas dimensions and visual style",
        ));
    }
    let quad = logical_quad(graph, layer, size.width, size.height)
        .map_err(|error| EditorError::new(error.code, error.message))?
        .ok_or_else(|| unsupported("Zero-sized transform targets cannot be edited"))?;
    Ok(Some(SampledElementGeometryV2 {
        intrinsic_size: size,
        quad,
    }))
}
