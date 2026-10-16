//! Fixed Classic media-frame facts. Runtime dimensions are observations, not
//! permission to replace the revision, source clock or animation state.
use super::*;
pub use opencut_contracts::visual_frame_v4::*;
use std::collections::BTreeMap;

pub const CLASSIC_VISUAL_PROFILE_V4: &str = "classic-v31-gpu-normal-frame";

#[derive(Clone, Debug)]
pub struct PreparedVisualFrameV4 {
    graph: CompositionV4,
    identity: VisualFrameIdentityV4,
}
impl PreparedVisualFrameV4 {
    pub fn identity(&self) -> &VisualFrameIdentityV4 {
        &self.identity
    }
    pub fn graph(&self) -> &CompositionV4 {
        &self.graph
    }
    pub fn resources(&self) -> &[CasResourceV4] {
        &self.graph.resources
    }
    pub fn layers(&self) -> &[CompiledLayerV4] {
        &self.graph.layers
    }
}

fn f32_value(value: f64) -> Result<f32, CompositionError> {
    let value = value as f32;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(unsupported("Classic GPU uniform exceeds finite f32"))
    }
}
fn rgba(color: &ColorV4) -> Result<[f32; 4], CompositionError> {
    let values = [color.red, color.green, color.blue, color.alpha];
    if color.encoding != ColorEncodingV4::Srgb
        || values
            .iter()
            .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
    {
        return Err(unsupported(
            "Classic media frame requires normalized sRGB background",
        ));
    }
    Ok([
        values[0] as f32,
        values[1] as f32,
        values[2] as f32,
        values[3] as f32,
    ])
}
fn covers_request(request: &Time, timestamp: f64, duration: f64) -> Result<bool, CompositionError> {
    let request = time(request)?;
    let seconds = request.ticks() as f64 / f64::from(request.timebase());
    let end = timestamp + duration;
    // Browser demux timestamps are IEEE seconds observations. A preceding frame
    // with a gap is not held past its own half-open presentation interval.
    Ok(timestamp.is_finite()
        && duration.is_finite()
        && duration > 0.0
        && end.is_finite()
        && timestamp <= seconds
        && seconds < end)
}
pub fn prepare_visual_frame_v4(
    graph: &CompositionV4,
) -> Result<PreparedVisualFrameV4, CompositionError> {
    reject_legacy_synthesis_fonts(&graph.fonts)?;
    if graph.scope != CompositionScope::Visual
        || graph.format.profile != FormatProfileV4::ClassicGlobal
        || !graph.audio.is_empty()
        || !graph.fonts.is_empty()
        || !graph.transitions.is_empty()
    {
        return Err(unsupported(
            "Classic media frame excludes legacy/audio/fonts/transitions",
        ));
    }
    if graph.project_id.is_empty()
        || graph.sequence_id.is_empty()
        || graph.revision.is_empty()
        || time(&graph.time)?.ticks() < 0
        || graph.format.canvas_size.width == 0
        || graph.format.canvas_size.height == 0
    {
        return Err(invalid("Invalid fixed visual frame identity"));
    }
    match &graph.background {
        CompiledBackgroundV4::Color { color } => {
            rgba(color)?;
        }
        _ => return Err(unsupported("Classic media frame excludes blur background")),
    }
    let mut ids = HashSet::new();
    let mut required = HashSet::new();
    for layer in &graph.layers {
        if layer.element_id.is_empty() || !ids.insert(&layer.element_id) {
            return Err(invalid("Duplicate visual layer identity"));
        }
        let style = layer
            .visual
            .as_ref()
            .ok_or_else(|| unsupported("Visual layer style missing"))?;
        if style.profile != VisualProfileV4::ClassicCanvasV31
            || style.blend_mode != BlendModeV4::Normal
            || style.effects.iter().any(|e| e.enabled)
            || style.masks.iter().any(|m| m.enabled)
        {
            return Err(unsupported(format!(
                "Layer {} requires unsupported visual capability",
                layer.element_id
            )));
        }
        for value in [
            style.transform.position_x_px,
            style.transform.position_y_px,
            style.transform.scale_x,
            style.transform.scale_y,
            style.transform.rotation_degrees,
            style.effective_opacity,
        ] {
            f32_value(value)?;
        }
        let asset_id = match &layer.content {
            SampledContentV4::Image { asset_id } => asset_id,
            SampledContentV4::Video {
                asset_id,
                stream,
                source,
                ..
            } => {
                if stream.is_some()
                    || source.point_profile != SourcePointProfileV4::ClassicVideoRoundedTicks
                {
                    return Err(unsupported(
                        "Classic video selects primary browser track and rounded source point",
                    ));
                }
                time(&source.source_time)?;
                if source.sample == SourceReadStateV4::NeedsResourceProbe {
                    return Err(unsupported(
                        "Video requires observed source extent before execution",
                    ));
                }
                asset_id
            }
            _ => {
                return Err(unsupported(format!(
                    "Layer {} is not video or image",
                    layer.element_id
                )));
            }
        };
        required.insert(asset_id.as_str());
    }
    let mut resources = HashSet::new();
    for resource in &graph.resources {
        if !resources.insert(resource.asset_id.as_str())
            || !required.contains(resource.asset_id.as_str())
        {
            return Err(invalid(
                "Visual resources must exactly cover participating layers",
            ));
        }
    }
    if resources != required {
        return Err(invalid("Visual resource binding missing"));
    }
    Ok(PreparedVisualFrameV4 {
        identity: VisualFrameIdentityV4 {
            project_id: graph.project_id.clone(),
            revision: graph.revision.clone(),
            sequence_id: graph.sequence_id.clone(),
            time: graph.time.clone(),
            width: graph.format.canvas_size.width,
            height: graph.format.canvas_size.height,
            profile: CLASSIC_VISUAL_PROFILE_V4.into(),
        },
        graph: graph.clone(),
    })
}

pub fn bind_visual_sources_v4(
    prepared: &PreparedVisualFrameV4,
    decoded: &[DecodedVisualSourceV4],
) -> Result<BoundVisualFrameV4, CompositionError> {
    let mut observations = BTreeMap::new();
    for value in decoded {
        if observations.insert(value.element_id(), value).is_some() {
            return Err(invalid("Duplicate decoded element"));
        }
    }
    if observations.len() != prepared.layers().len() {
        return Err(invalid(
            "Decoded observations must cover the complete frame",
        ));
    }
    let mut layers = Vec::new();
    let mut ordered = Vec::new();
    for layer in prepared.layers() {
        let observation = observations
            .get(layer.element_id.as_str())
            .ok_or_else(|| invalid("Decoded observation missing"))?;
        let (asset_id, size) = match (&layer.content, *observation) {
            (
                SampledContentV4::Image { asset_id },
                DecodedVisualSourceV4::ReadyImage { width, height, .. },
            ) => (asset_id, Some((*width, *height))),
            (
                SampledContentV4::Video {
                    asset_id,
                    stream,
                    source,
                    ..
                },
                DecodedVisualSourceV4::ReadyVideo {
                    requested_time,
                    selected_video_stream,
                    actual_timestamp_seconds,
                    frame_duration_seconds,
                    width,
                    height,
                    ..
                },
            ) if requested_time == &source.source_time
                && selected_video_stream == stream
                && covers_request(
                    requested_time,
                    *actual_timestamp_seconds,
                    *frame_duration_seconds,
                )?
                && source.sample == SourceReadStateV4::InBounds =>
            {
                (asset_id, Some((*width, *height)))
            }
            (
                SampledContentV4::Video {
                    asset_id, source, ..
                },
                DecodedVisualSourceV4::NoFrame {
                    requested_time,
                    reason,
                    ..
                },
            ) if requested_time == &source.source_time
                && match reason {
                    VisualNoFrameReasonV4::OutsideReadWindow => {
                        source.sample != SourceReadStateV4::InBounds
                    }
                    VisualNoFrameReasonV4::Eof | VisualNoFrameReasonV4::TimestampGap => {
                        source.sample == SourceReadStateV4::InBounds
                    }
                } =>
            {
                (asset_id, None)
            }
            _ => {
                return Err(invalid(
                    "Decoded kind, request point, selection or timestamp differs from fixed layer",
                ));
            }
        };
        let quad = if let Some((width, height)) = size {
            if width == 0 || height == 0 {
                return Err(invalid("Actual decoded dimensions must be positive"));
            }
            let style = layer.visual.as_ref().expect("prepared style");
            let transform = style.transform;
            let contain = (f64::from(prepared.identity.width) / f64::from(width))
                .min(f64::from(prepared.identity.height) / f64::from(height));
            let scaled_width = f64::from(width) * contain * transform.scale_x;
            let scaled_height = f64::from(height) * contain * transform.scale_y;
            let quad = VisualQuadV4 {
                center_x: f32_value(
                    f64::from(prepared.identity.width) / 2.0 + transform.position_x_px,
                )?,
                center_y: f32_value(
                    f64::from(prepared.identity.height) / 2.0 + transform.position_y_px,
                )?,
                width: f32_value(scaled_width.abs())?,
                height: f32_value(scaled_height.abs())?,
                rotation_radians: f32_value(transform.rotation_degrees)?.to_radians(),
                flip_x: scaled_width < 0.0,
                flip_y: scaled_height < 0.0,
                opacity: f32_value(style.effective_opacity)?,
            };
            if quad.width == 0.0 || quad.height == 0.0 {
                None
            } else {
                Some(quad)
            }
        } else {
            None
        };
        layers.push(BoundVisualLayerV4 {
            element_id: layer.element_id.clone(),
            asset_id: asset_id.clone(),
            quad,
        });
        ordered.push((**observation).clone());
    }
    let CompiledBackgroundV4::Color { color } = &prepared.graph.background else {
        unreachable!("prepared color")
    };
    Ok(BoundVisualFrameV4 {
        identity: prepared.identity.clone(),
        clear: [0.0, 0.0, 0.0, 1.0],
        background: rgba(color)?,
        layers,
        decoded: ordered,
    })
}

#[cfg(test)]
mod tests;
