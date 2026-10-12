use super::*;
use std::collections::HashSet;

pub(super) fn asset_id(layer: &CompiledLayerV4) -> Option<&str> {
    match &layer.content {
        SampledContentV4::Video { asset_id, .. } | SampledContentV4::Image { asset_id } => {
            Some(asset_id)
        }
        _ => None,
    }
}
pub(super) fn visual_resource_used(graph: &CompositionV4, id: &str) -> bool {
    graph.layers.iter().any(|layer| asset_id(layer) == Some(id))
}

pub(super) fn validate(graph: &CompositionV4) -> Result<(), ProjectRenderError> {
    if graph.revision.is_empty() || media_time(&graph.time)?.ticks < 0 {
        return Err(error(
            "INVALID_PROJECT",
            "IR4 frame requires fixed revision and nonnegative time",
        ));
    }
    media_time(&graph.time)?;
    if graph.format.canvas_size.width == 0
        || graph.format.canvas_size.height == 0
        || graph.format.fps.numerator == 0
        || graph.format.fps.denominator == 0
    {
        return Err(error("INVALID_PROJECT", "IR4 frame format is invalid"));
    }
    match graph.background {
        CompiledBackgroundV4::Color { color } if color.alpha == 1.0 => {
            rgba(&color, "background")?;
        }
        _ => {
            return Err(unsupported(
                "background",
                "transparent or main-track blur background",
            ));
        }
    }
    let mut ids = HashSet::new();
    for layer in &graph.layers {
        if layer.element_id.is_empty() || !ids.insert(layer.element_id.as_str()) {
            return Err(error(
                "INVALID_PROJECT",
                "IR4 layer identities must be unique",
            ));
        }
        let style = layer
            .visual
            .as_ref()
            .ok_or_else(|| unsupported(&layer.element_id, "missing visual style"))?;
        if style.profile != VisualProfileV4::LegacyCompositionV3 {
            return Err(unsupported(&layer.element_id, "Classic visual profile"));
        }
        if style.blend_mode != BlendModeV4::Normal {
            return Err(unsupported(&layer.element_id, "blend mode"));
        }
        let t = style.transform;
        if ![
            t.position_x_px,
            t.position_y_px,
            t.scale_x,
            t.scale_y,
            t.rotation_degrees,
            style.effective_opacity,
        ]
        .iter()
        .all(|v| v.is_finite())
            || t.scale_x < 0.0
            || t.scale_y < 0.0
            || !(0.0..=1.0).contains(&style.effective_opacity)
        {
            return Err(unsupported(
                &layer.element_id,
                "negative scale or unbounded/nonfinite visual values",
            ));
        }
        match &layer.content {
            SampledContentV4::Video { source, .. } => {
                let point = opencut_time::RationalTime::try_from(&source.source_time)
                    .map_err(project_time_error)?;
                if source.sample != SourceReadStateV4::InBounds {
                    return Err(unsupported(
                        &layer.element_id,
                        "unresolved source extent or EOF policy",
                    ));
                }
                let BoundedReadWindowV4::Known {
                    resource_extent,
                    range: Some(range),
                } = &source.read_window
                else {
                    return Err(unsupported(
                        &layer.element_id,
                        "unverified decoder read window",
                    ));
                };
                let parse =
                    |t: &Time| opencut_time::RationalTime::try_from(t).map_err(project_time_error);
                let range_end = opencut_time::range_end(range).map_err(project_time_error)?;
                if point.ticks() < 0
                    || point.cmp_exact(parse(&range.start)?) == std::cmp::Ordering::Less
                    || point.cmp_exact(parse(&range_end)?) != std::cmp::Ordering::Less
                    || point.cmp_exact(parse(resource_extent)?) != std::cmp::Ordering::Less
                {
                    return Err(unsupported(
                        &layer.element_id,
                        "source point outside bounded decoder window",
                    ));
                }
            }
            SampledContentV4::Image { .. } => {}
            SampledContentV4::Graphic { graphic } => {
                if t.scale_x != 1.0 || t.scale_y != 1.0 || graphic.stroke.width_px != 0.0 {
                    return Err(unsupported(
                        &layer.element_id,
                        "scaled or stroked legacy graphic",
                    ));
                }
                primitive(graphic, &layer.element_id)?;
                rgba(&graphic.fill, &layer.element_id)?;
            }
            _ => {
                return Err(unsupported(
                    &layer.element_id,
                    "text/caption/sticker/effect-layer rasterization",
                ));
            }
        }
        for effect in style.effects.iter().filter(|e| e.enabled) {
            effect_kind(&effect.effect, &effect.id)?;
        }
        for mask in style.masks.iter().filter(|m| m.enabled) {
            mask_node(mask, &layer.element_id)?;
        }
    }
    let mut incoming = HashSet::new();
    for transition in &graph.transitions {
        if !incoming.insert(&transition.incoming_element_id) {
            return Err(unsupported(
                &transition.id,
                "multiple transitions on one incoming layer",
            ));
        }
        if transition.profile != TransitionKindV4::LegacyCrossfadeV3
            || transition.outgoing_weight != 1.0
            || !(0.0..=1.0).contains(&transition.incoming_weight)
        {
            return Err(unsupported(&transition.id, "transition profile or weight"));
        }
        for id in [
            &transition.outgoing_element_id,
            &transition.incoming_element_id,
        ] {
            if !graph
                .layers
                .iter()
                .any(|l| l.element_id == *id && matches!(l.content, SampledContentV4::Video { .. }))
            {
                return Err(unsupported(
                    &transition.id,
                    "missing video transition participant",
                ));
            }
        }
    }
    Ok(())
}

fn rgba(color: &ColorV4, node: &str) -> Result<RgbaV3, ProjectRenderError> {
    if color.encoding != ColorEncodingV4::Srgb
        || ![color.red, color.green, color.blue, color.alpha]
            .iter()
            .all(|c| (0.0..=1.0).contains(c))
    {
        return Err(unsupported(node, "linear-sRGB or invalid color"));
    }
    let byte = |c: f64| (c * 255.0).round() as u8;
    Ok(RgbaV3 {
        red: byte(color.red),
        green: byte(color.green),
        blue: byte(color.blue),
        alpha: byte(color.alpha),
    })
}
fn primitive(graphic: &GraphicV4, node: &str) -> Result<ShapePrimitiveV3, ProjectRenderError> {
    match graphic.geometry {
        GraphicGeometryV4::LegacyRectangle {
            width_px,
            height_px,
        } if width_px.is_finite() && height_px.is_finite() && width_px > 0.0 && height_px > 0.0 => {
            Ok(ShapePrimitiveV3::Rectangle {
                width_px,
                height_px,
            })
        }
        GraphicGeometryV4::LegacyEllipse {
            radius_x_px,
            radius_y_px,
        } if radius_x_px.is_finite()
            && radius_y_px.is_finite()
            && radius_x_px > 0.0
            && radius_y_px > 0.0 =>
        {
            Ok(ShapePrimitiveV3::Ellipse {
                radius_x_px,
                radius_y_px,
            })
        }
        _ => Err(unsupported(node, "graphic geometry")),
    }
}
fn effect_kind(kind: &EffectKindV4, node: &str) -> Result<VisualEffectV3, ProjectRenderError> {
    match *kind {
        EffectKindV4::LegacyGaussianBlur { radius_px } if (0.0..=128.0).contains(&radius_px) => {
            Ok(VisualEffectV3::GaussianBlur { radius_px })
        }
        EffectKindV4::LegacyColorAdjust {
            exposure_ev,
            contrast,
            saturation,
        } if (-8.0..=8.0).contains(&exposure_ev)
            && (0.0..=4.0).contains(&contrast)
            && (0.0..=4.0).contains(&saturation) =>
        {
            Ok(VisualEffectV3::ColorAdjust {
                exposure_ev,
                contrast,
                saturation,
            })
        }
        _ => Err(unsupported(node, "effect kind/parameter range")),
    }
}
fn mask_node(mask: &MaskV4, target: &str) -> Result<MaskNodeV3, ProjectRenderError> {
    if mask.stroke.width_px != 0.0 {
        return Err(unsupported(&mask.id, "mask stroke"));
    }
    let feather = match mask.feather {
        MaskFeatherV4::LegacyCanvasFraction { value } if value.is_finite() && value >= 0.0 => value,
        _ => return Err(unsupported(&mask.id, "mask feather unit")),
    };
    let (x, y, width, height, ellipse) = match mask.shape {
        MaskShapeV4::LegacyRectangle {
            x,
            y,
            width,
            height,
        } => (x, y, width, height, false),
        MaskShapeV4::LegacyEllipse {
            x,
            y,
            width,
            height,
        } => (x, y, width, height, true),
        _ => return Err(unsupported(&mask.id, "mask coordinate profile")),
    };
    if ![x, y, width, height].iter().all(|v| v.is_finite())
        || x < 0.0
        || y < 0.0
        || width <= 0.0
        || height <= 0.0
        || x + width > 1.0
        || y + height > 1.0
    {
        return Err(unsupported(&mask.id, "mask geometry range"));
    }
    let shape = if ellipse {
        MaskShapeV3::Ellipse {
            x,
            y,
            width,
            height,
        }
    } else {
        MaskShapeV3::Rectangle {
            x,
            y,
            width,
            height,
        }
    };
    Ok(MaskNodeV3 {
        target_id: target.into(),
        shape,
        inverted: mask.inverted,
        feather,
    })
}

/// Lower sampled nodes, not a project aggregate. Videos must already be decoded
/// at their IR source point. Backend clock zero is local to this single frame.
pub(super) fn lower(
    graph: &CompositionV4,
    inputs: &HashMap<String, PathBuf>,
    output: PathBuf,
    overwrite: bool,
) -> Result<TimelineFrameRenderRequestV3, ProjectRenderError> {
    validate(graph)?;
    let start = MediaTime {
        ticks: 0,
        timebase: 1,
    };
    let duration = MediaTime {
        ticks: i64::from(graph.format.fps.denominator),
        timebase: graph.format.fps.numerator,
    };
    let mut base = TimelineExportRequest {
        output,
        container: Container::Mp4,
        video: VideoOutput {
            width: graph.format.canvas_size.width,
            height: graph.format.canvas_size.height,
            frame_rate: FrameRate {
                numerator: graph.format.fps.numerator,
                denominator: graph.format.fps.denominator,
            },
            aspect_mode: AspectMode::Fit,
            codec: VideoCodec::H264,
            crf: None,
        },
        duration,
        video_clips: vec![],
        image_clips: vec![],
        audio_clips: vec![],
        captions: vec![],
        overwrite,
    };
    let mut visual = CompiledVisualGraphV3 {
        targets: vec![],
        shapes: vec![],
        effects: vec![],
        masks: vec![],
        transitions: vec![],
    };
    let CompiledBackgroundV4::Color { color } = graph.background else {
        unreachable!("validated color background")
    };
    // A private target identity is unique even for a malicious layer named so.
    let mut bg = "__ir4_background".to_string();
    while graph.layers.iter().any(|l| l.element_id == bg) {
        bg.push('_');
    }
    visual.shapes.push(CompiledShapeV3 {
        id: bg.clone(),
        start,
        duration,
        primitive: ShapePrimitiveV3::Rectangle {
            width_px: f64::from(base.video.width) + 2.0,
            height_px: f64::from(base.video.height) + 2.0,
        },
        fill: rgba(&color, "background")?,
        position_x: 0.0,
        position_y: 0.0,
        rotation_degrees: 0.0,
        opacity: 1.0,
    });
    visual.targets.push(VisualTargetBindingV3 {
        target_id: bg,
        kind: VisualTargetKindV3::Shape,
        index: 0,
        z_index: 0,
        layer_order: 0,
    });
    for (order, layer) in graph.layers.iter().enumerate() {
        let style = layer.visual.as_ref().expect("validated visual");
        let t = style.transform;
        let opacity = graph
            .transitions
            .iter()
            .filter(|tr| tr.incoming_element_id == layer.element_id)
            .fold(style.effective_opacity, |value, tr| {
                value * tr.incoming_weight
            });
        let (kind, index) = match &layer.content {
            SampledContentV4::Video { .. } | SampledContentV4::Image { .. } => {
                let input = inputs.get(&layer.element_id).ok_or_else(|| {
                    error(
                        "ASSET_RESOLUTION_FAILED",
                        format!("Missing prepared frame {}", layer.element_id),
                    )
                })?;
                let index = base.image_clips.len();
                base.image_clips.push(TimelineImageClip {
                    input: input.clone(),
                    timeline_start: start,
                    timeline_duration: duration,
                    stream: None,
                    opacity: opacity as f32,
                    position_x: t.position_x_px,
                    position_y: t.position_y_px,
                    scale_x: t.scale_x,
                    scale_y: t.scale_y,
                    rotation_degrees: t.rotation_degrees,
                    z_index: 0,
                    curves: TimelineVisualCurves::default(),
                });
                (VisualTargetKindV3::ImageClip, index)
            }
            SampledContentV4::Graphic { graphic } => {
                let index = visual.shapes.len();
                visual.shapes.push(CompiledShapeV3 {
                    id: layer.element_id.clone(),
                    start,
                    duration,
                    primitive: primitive(graphic, &layer.element_id)?,
                    fill: rgba(&graphic.fill, &layer.element_id)?,
                    position_x: t.position_x_px,
                    position_y: t.position_y_px,
                    rotation_degrees: t.rotation_degrees,
                    opacity,
                });
                (VisualTargetKindV3::Shape, index)
            }
            _ => unreachable!("validated content"),
        };
        let layer_order = u32::try_from(order + 1)
            .map_err(|_| error("INVALID_PROJECT", "Too many IR4 frame layers"))?;
        visual.targets.push(VisualTargetBindingV3 {
            target_id: layer.element_id.clone(),
            kind,
            index,
            z_index: 0,
            layer_order,
        });
        for effect in style.effects.iter().filter(|e| e.enabled) {
            visual.effects.push(EffectNodeV3 {
                target_id: layer.element_id.clone(),
                effect: effect_kind(&effect.effect, &effect.id)?,
            });
        }
        for mask in style.masks.iter().filter(|m| m.enabled) {
            visual.masks.push(mask_node(mask, &layer.element_id)?);
        }
    }
    Ok(TimelineFrameRenderRequestV3 {
        timeline: TimelineExportRequestV3 { base, visual },
        sequence_time: start,
    })
}
