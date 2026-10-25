//! Fixed-source authoring bounds use the sampled IR and the renderer's shared
//! double-precision contain transform. No media/font I/O or static-scene fallback.
use opencut_composition::scene::{CompiledLayer, SampledContent, SceneComposition, logical_quad};
use opencut_contracts::{
    EditorError, element_transform::SampledElementGeometry, scene::CanvasSize,
};

fn unsupported(message: &str) -> EditorError {
    EditorError::new("UNSUPPORTED_VISUAL_FEATURE", message)
}

pub(super) fn sampled_geometry(
    graph: &SceneComposition,
    layer: &CompiledLayer,
) -> Result<Option<SampledElementGeometry>, EditorError> {
    let size = match &layer.content {
        SampledContent::Text { .. } => return Ok(None),
        SampledContent::Graphic { graphic } => graphic.source_size,
        SampledContent::Video { asset_id, .. }
        | SampledContent::Image { asset_id }
        | SampledContent::Sticker { asset_id, .. } => {
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
                CanvasSize {
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
    Ok(Some(SampledElementGeometry {
        intrinsic_size: size,
        quad,
    }))
}
