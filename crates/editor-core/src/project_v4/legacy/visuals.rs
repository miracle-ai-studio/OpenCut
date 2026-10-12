//! Legacy shape pixels, per-target stack order, masks and global caption layout.
use super::*;

pub(super) fn rgba(red: u8, green: u8, blue: u8, alpha: u8) -> ColorV4 {
    ColorV4 {
        encoding: ColorEncodingV4::Srgb,
        red: f64::from(red) / 255.0,
        green: f64::from(green) / 255.0,
        blue: f64::from(blue) / 255.0,
        alpha: f64::from(alpha) / 255.0,
    }
}
fn no_stroke() -> StrokeV4 {
    StrokeV4 {
        color: rgba(0, 0, 0, 0),
        width_px: 0.0,
        align: StrokeAlignV4::Center,
    }
}

pub(super) fn apply_graph(
    sequence: &Sequence,
    graph: &SequenceVisualV3,
    tracks: &mut [TrackV4],
    context: &mut Context,
) {
    for shape in &graph.shapes {
        let Some(track) = tracks.iter_mut().find(|track| track.id == shape.track_id) else {
            continue;
        };
        let geometry = match shape.primitive {
            ShapePrimitive::Rectangle {
                width_px,
                height_px,
            } => GraphicGeometryV4::LegacyRectangle {
                width_px,
                height_px,
            },
            ShapePrimitive::Ellipse {
                radius_x_px,
                radius_y_px,
            } => GraphicGeometryV4::LegacyEllipse {
                radius_x_px,
                radius_y_px,
            },
        };
        // Legacy geometry declares actual pixel dimensions independently; the
        // canvas-sized local box documents its original coordinate frame. It
        // must not rescale or round the stored floating-point geometry.
        track.elements.push(TimelineElementV4 {
            id: shape.id.clone(),
            name: "Shape".into(),
            timeline_range: shape.timeline_range.clone(),
            source_span: ElementSourceSpanV4::ClassicTrimmed {
                trim_start: elements::zero(),
                trim_end: elements::zero(),
                source_duration: Some(shape.timeline_range.duration.clone()),
            },
            enabled: true,
            content: ElementContentV4::Graphic {
                graphic: GraphicV4 {
                    geometry,
                    source_size: CanvasSizeV4 {
                        width: sequence.width,
                        height: sequence.height,
                    },
                    fill: rgba(
                        shape.fill.red,
                        shape.fill.green,
                        shape.fill.blue,
                        shape.fill.alpha,
                    ),
                    stroke: no_stroke(),
                },
            },
            visual: Some(VisualStyleV4 {
                profile: VisualProfileV4::LegacyCompositionV3,
                transform: TransformV4 {
                    position_x_px: shape.position_x,
                    position_y_px: shape.position_y,
                    scale_x: 1.0,
                    scale_y: 1.0,
                    rotation_degrees: shape.rotation_degrees,
                },
                opacity: shape.opacity,
                blend_mode: BlendModeV4::Normal,
                effects: Vec::new(),
                masks: Vec::new(),
            }),
            channels: Vec::new(),
        });
        context
            .report
            .normalizations
            .push(LegacyNormalization::ShapeOrderMadeExplicit {
                sequence_id: sequence.id.clone(),
                shape_id: shape.id.clone(),
                track_id: track.id.clone(),
            });
    }
    for node in &graph.effects {
        if let Some(style) = style(tracks, &node.target_id) {
            let effect = match node.effect {
                VisualEffect::ColorAdjust {
                    exposure_ev,
                    contrast,
                    saturation,
                } => EffectKindV4::LegacyColorAdjust {
                    exposure_ev,
                    contrast,
                    saturation,
                },
                VisualEffect::GaussianBlur { radius_px } => {
                    EffectKindV4::LegacyGaussianBlur { radius_px }
                }
            };
            style.effects.push(EffectV4 {
                id: node.id.clone(),
                enabled: true,
                effect,
            });
        }
    }
    for node in &graph.masks {
        if let Some(style) = style(tracks, &node.target_id) {
            let shape = match node.shape {
                MaskShape::Rectangle {
                    x,
                    y,
                    width,
                    height,
                } => MaskShapeV4::LegacyRectangle {
                    x,
                    y,
                    width,
                    height,
                },
                MaskShape::Ellipse {
                    x,
                    y,
                    width,
                    height,
                } => MaskShapeV4::LegacyEllipse {
                    x,
                    y,
                    width,
                    height,
                },
            };
            style.masks.push(MaskV4 {
                id: node.id.clone(),
                enabled: true,
                shape,
                inverted: node.inverted,
                feather: MaskFeatherV4::LegacyCanvasFraction {
                    value: node.feather,
                },
                stroke: no_stroke(),
            });
        }
    }
}
fn style<'a>(tracks: &'a mut [TrackV4], id: &str) -> Option<&'a mut VisualStyleV4> {
    tracks
        .iter_mut()
        .flat_map(|track| &mut track.elements)
        .find(|e| e.id == id)
        .and_then(|e| e.visual.as_mut())
}

pub(super) fn caption(
    cue: &CaptionCue,
    track_id: &str,
    path: &str,
    context: &mut Context,
) -> Option<CaptionV4> {
    let color = caption_color(&cue.style.color);
    if color.is_none() {
        context.issue(
            format!("{path}.style.color"),
            LegacyConversionIssueKind::UnsupportedCaptionColor {
                value: cue.style.color.clone(),
            },
        );
    }
    let background_color = cue.style.background_color.as_ref().and_then(|value| {
        let color = caption_color(value);
        if color.is_none() {
            context.issue(
                format!("{path}.style.background_color"),
                LegacyConversionIssueKind::UnsupportedCaptionColor {
                    value: value.clone(),
                },
            );
        }
        color
    });
    Some(CaptionV4 {
        id: cue.id.clone(),
        track_id: track_id.into(),
        timeline_range: cue.timeline_range.clone(),
        text: cue.text.clone(),
        source_anchor: cue.source_anchor.clone(),
        style: CaptionStyleV4 {
            font: FontBindingV4::UnresolvedFamily {
                family: cue.style.font_family.clone(),
                policy: cue.style.font_policy.clone(),
            },
            font_size: FontSizeV4::CanvasPixels {
                value: cue.style.font_size,
            },
            color: color?,
            background_color,
            max_width_px: cue.style.max_width,
            line_height_px: cue.style.line_height,
            text_align: match cue.style.text_align {
                TextAlign::Left => TextAlignV4::Left,
                TextAlign::Center => TextAlignV4::Center,
                TextAlign::Right => TextAlignV4::Right,
            },
            layout: CaptionLayoutProfileV4::LegacyCaptionV2,
        },
    })
}

/// Matches the old media caption parser exactly, including f32 alpha-to-u8
/// rounding. Keeping an unquantized alpha would change historical pixels.
fn caption_color(value: &str) -> Option<ColorV4> {
    let (base, alpha) = match value.split_once('@') {
        Some((base, alpha)) => (base, alpha.parse::<f32>().ok()?),
        None => (value, 1.0),
    };
    if !(0.0..=1.0).contains(&alpha) {
        return None;
    }
    let color = match base.to_ascii_lowercase().as_str() {
        "white" => (255, 255, 255),
        "black" => (0, 0, 0),
        "red" => (255, 0, 0),
        "green" => (0, 128, 0),
        "blue" => (0, 0, 255),
        hex if hex.is_ascii()
            && hex.len() == 7
            && hex.starts_with('#')
            && hex.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit) =>
        {
            (
                u8::from_str_radix(&hex[1..3], 16).ok()?,
                u8::from_str_radix(&hex[3..5], 16).ok()?,
                u8::from_str_radix(&hex[5..7], 16).ok()?,
            )
        }
        _ => return None,
    };
    Some(rgba(
        color.0,
        color.1,
        color.2,
        (alpha * 255.0).round() as u8,
    ))
}
