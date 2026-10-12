//! Exhaustive typed field setters; no sampling or document authority.
use super::*;
pub(super) fn mismatch() -> CompositionError {
    unsupported("Animation binding has no matching V4 execution field")
}
pub(super) fn set_gain(
    gain: &mut GainV4,
    parameter: ScalarParameterV4,
    value: f64,
) -> Result<(), CompositionError> {
    match (gain, parameter) {
        (GainV4::LinearLegacy { value: output }, ScalarParameterV4::GainLinearLegacy)
        | (GainV4::Decibels { value: output }, ScalarParameterV4::GainDecibels) => {
            *output = value;
            Ok(())
        }
        _ => Err(mismatch()),
    }
}
pub(super) fn font_size(
    size: &mut FontSizeV4,
    classic: bool,
    value: f64,
) -> Result<(), CompositionError> {
    match (size, classic) {
        (FontSizeV4::ClassicHeight90 { value: to }, true)
        | (FontSizeV4::CanvasPixels { value: to }, false) => {
            *to = value;
            Ok(())
        }
        _ => Err(mismatch()),
    }
}
pub(super) fn text_property(
    text: &mut TextV4,
    p: ScalarParameterV4,
    v: f64,
) -> Result<(), CompositionError> {
    use ScalarParameterV4::*;
    match p {
        FontSizeClassicHeight90 => return font_size(&mut text.font_size, true, v),
        FontSizeCanvasPixels => return font_size(&mut text.font_size, false, v),
        LetterSpacingPx => text.letter_spacing_px = v,
        LineHeightMultiplier => text.line_height_multiplier = v,
        TextBackgroundPaddingXAtSize15 => text.background.padding_x_at_size15 = v,
        TextBackgroundPaddingYAtSize15 => text.background.padding_y_at_size15 = v,
        TextBackgroundOffsetXPx => text.background.offset_x_px = v,
        TextBackgroundOffsetYPx => text.background.offset_y_px = v,
        TextBackgroundCornerRadiusPercent => text.background.corner_radius_percent = v,
        _ => return Err(mismatch()),
    }
    Ok(())
}
pub(super) fn graphic_property(
    graphic: &mut GraphicV4,
    p: ScalarParameterV4,
    v: f64,
) -> Result<(), CompositionError> {
    use ScalarParameterV4::*;
    match (p, &mut graphic.geometry) {
        (GraphicStrokeWidthPx, _) => graphic.stroke.width_px = v,
        (
            GraphicCornerRadiusPercent,
            GraphicGeometryV4::Rectangle {
                corner_radius_percent,
            }
            | GraphicGeometryV4::Polygon {
                corner_radius_percent,
                ..
            },
        ) => *corner_radius_percent = v,
        (GraphicSides, GraphicGeometryV4::Polygon { sides, .. }) => *sides = v,
        (GraphicPoints, GraphicGeometryV4::Star { points, .. }) => *points = v,
        (GraphicDepthPercent, GraphicGeometryV4::Star { depth_percent, .. }) => *depth_percent = v,
        _ => return Err(mismatch()),
    }
    Ok(())
}
pub(super) fn effect_property(
    effect: &mut EffectV4,
    p: ScalarParameterV4,
    v: f64,
) -> Result<(), CompositionError> {
    use ScalarParameterV4::*;
    match (p, &mut effect.effect) {
        (ClassicBlurIntensity, EffectKindV4::ClassicBlur { intensity }) => *intensity = v,
        (LegacyBlurRadiusPx, EffectKindV4::LegacyGaussianBlur { radius_px }) => *radius_px = v,
        (LegacyExposureEv, EffectKindV4::LegacyColorAdjust { exposure_ev, .. }) => *exposure_ev = v,
        (LegacyContrast, EffectKindV4::LegacyColorAdjust { contrast, .. }) => *contrast = v,
        (LegacySaturation, EffectKindV4::LegacyColorAdjust { saturation, .. }) => *saturation = v,
        _ => return Err(mismatch()),
    }
    Ok(())
}
pub(super) fn box_geometry(shape: &mut MaskShapeV4) -> Option<&mut BoxMaskGeometryV4> {
    match shape {
        MaskShapeV4::CinematicBars { geometry }
        | MaskShapeV4::Rectangle { geometry }
        | MaskShapeV4::Ellipse { geometry }
        | MaskShapeV4::Heart { geometry }
        | MaskShapeV4::Diamond { geometry }
        | MaskShapeV4::Star { geometry } => Some(geometry),
        _ => None,
    }
}
pub(super) fn mask_property(
    mask: &mut MaskV4,
    p: ScalarParameterV4,
    v: f64,
) -> Result<(), CompositionError> {
    use ScalarParameterV4::*;
    match p {
        MaskFeatherClassicPixels => match &mut mask.feather {
            MaskFeatherV4::ClassicPixels { value } => *value = v,
            _ => return Err(mismatch()),
        },
        MaskFeatherLegacyCanvasFraction => match &mut mask.feather {
            MaskFeatherV4::LegacyCanvasFraction { value } => *value = v,
            _ => return Err(mismatch()),
        },
        MaskStrokeWidthPx => mask.stroke.width_px = v,
        MaskWidth | MaskHeight => {
            let geometry = box_geometry(&mut mask.shape).ok_or_else(mismatch)?;
            if p == MaskWidth {
                geometry.width = v
            } else {
                geometry.height = v
            }
        }
        MaskCenterX | MaskCenterY | MaskRotationDegrees | MaskScale => {
            if let Some(g) = box_geometry(&mut mask.shape) {
                match p {
                    MaskCenterX => g.center_x = v,
                    MaskCenterY => g.center_y = v,
                    MaskRotationDegrees => g.rotation_degrees = v,
                    MaskScale => g.scale = v,
                    _ => unreachable!(),
                }
            } else {
                match &mut mask.shape {
                    MaskShapeV4::Split {
                        center_x,
                        center_y,
                        rotation_degrees,
                    } => match p {
                        MaskCenterX => *center_x = v,
                        MaskCenterY => *center_y = v,
                        MaskRotationDegrees => *rotation_degrees = v,
                        _ => return Err(mismatch()),
                    },
                    MaskShapeV4::Text { mask } => match p {
                        MaskCenterX => mask.center_x = v,
                        MaskCenterY => mask.center_y = v,
                        MaskRotationDegrees => mask.rotation_degrees = v,
                        MaskScale => mask.scale = v,
                        _ => unreachable!(),
                    },
                    MaskShapeV4::Freeform { path } => match p {
                        MaskCenterX => path.center_x = v,
                        MaskCenterY => path.center_y = v,
                        MaskRotationDegrees => path.rotation_degrees = v,
                        MaskScale => path.scale = v,
                        _ => unreachable!(),
                    },
                    _ => return Err(mismatch()),
                }
            }
        }
        MaskFontSizeClassicHeight90
        | MaskFontSizeCanvasPixels
        | MaskLetterSpacingPx
        | MaskLineHeightMultiplier => {
            let MaskShapeV4::Text { mask } = &mut mask.shape else {
                return Err(mismatch());
            };
            match p {
                MaskFontSizeClassicHeight90 => return font_size(&mut mask.font_size, true, v),
                MaskFontSizeCanvasPixels => return font_size(&mut mask.font_size, false, v),
                MaskLetterSpacingPx => mask.letter_spacing_px = v,
                MaskLineHeightMultiplier => mask.line_height_multiplier = v,
                _ => unreachable!(),
            }
        }
        _ => return Err(mismatch()),
    }
    Ok(())
}
pub(super) fn element_property(
    element: &mut TimelineElementV4,
    p: ScalarParameterV4,
    v: f64,
) -> Result<(), CompositionError> {
    use ScalarParameterV4::*;
    match p {
        PositionXPx | PositionYPx | ScaleX | ScaleY | RotationDegrees | Opacity => {
            let visual = element.visual.as_mut().ok_or_else(mismatch)?;
            match p {
                PositionXPx => visual.transform.position_x_px = v,
                PositionYPx => visual.transform.position_y_px = v,
                ScaleX => visual.transform.scale_x = v,
                ScaleY => visual.transform.scale_y = v,
                RotationDegrees => visual.transform.rotation_degrees = v,
                Opacity => visual.opacity = v,
                _ => unreachable!(),
            }
        }
        GainLinearLegacy | GainDecibels => {
            let audio = match &mut element.content {
                ElementContentV4::Video { audio, .. } | ElementContentV4::Audio { audio, .. } => {
                    audio
                }
                _ => return Err(mismatch()),
            };
            return set_gain(&mut audio.gain, p, v);
        }
        FontSizeClassicHeight90
        | FontSizeCanvasPixels
        | LetterSpacingPx
        | LineHeightMultiplier
        | TextBackgroundPaddingXAtSize15
        | TextBackgroundPaddingYAtSize15
        | TextBackgroundOffsetXPx
        | TextBackgroundOffsetYPx
        | TextBackgroundCornerRadiusPercent => {
            let ElementContentV4::Text { text } = &mut element.content else {
                return Err(mismatch());
            };
            return text_property(text, p, v);
        }
        GraphicStrokeWidthPx
        | GraphicCornerRadiusPercent
        | GraphicSides
        | GraphicPoints
        | GraphicDepthPercent => {
            let ElementContentV4::Graphic { graphic } = &mut element.content else {
                return Err(mismatch());
            };
            return graphic_property(graphic, p, v);
        }
        _ => return Err(mismatch()),
    }
    Ok(())
}

fn color_component(color: &mut ColorV4, component: ColorComponentV4, v: f64) {
    if component != ColorComponentV4::Alpha && color.encoding == ColorEncodingV4::Srgb {
        fn linear(v: f64) -> f64 {
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        }
        color.red = linear(color.red);
        color.green = linear(color.green);
        color.blue = linear(color.blue);
        color.encoding = ColorEncodingV4::LinearSrgb;
    }
    match component {
        ColorComponentV4::Red => color.red = v,
        ColorComponentV4::Green => color.green = v,
        ColorComponentV4::Blue => color.blue = v,
        ColorComponentV4::Alpha => color.alpha = v,
    }
}
fn element_color(
    element: &mut TimelineElementV4,
    p: ColorParameterV4,
) -> Result<&mut ColorV4, CompositionError> {
    match (p, &mut element.content) {
        (ColorParameterV4::TextColor, ElementContentV4::Text { text }) => Ok(&mut text.color),
        (ColorParameterV4::TextBackgroundColor, ElementContentV4::Text { text }) => {
            Ok(&mut text.background.color)
        }
        (ColorParameterV4::GraphicFill, ElementContentV4::Graphic { graphic }) => {
            Ok(&mut graphic.fill)
        }
        (ColorParameterV4::GraphicStroke, ElementContentV4::Graphic { graphic }) => {
            Ok(&mut graphic.stroke.color)
        }
        _ => Err(mismatch()),
    }
}
pub(super) fn apply(
    element: &mut TimelineElementV4,
    target: &ChannelTargetV4,
    sample: SampledChannelV4,
) -> Result<(), CompositionError> {
    match target {
        ChannelTargetV4::Element { .. } => match sample {
            SampledChannelV4::Scalar {
                binding: ScalarBindingV4::Property { parameter },
                physical,
                ..
            } => element_property(element, parameter, physical),
            SampledChannelV4::Scalar {
                binding:
                    ScalarBindingV4::ColorComponent {
                        parameter,
                        component,
                    },
                physical,
                ..
            } => {
                color_component(element_color(element, parameter)?, component, physical);
                Ok(())
            }
            SampledChannelV4::Discrete {
                parameter: DiscreteParameterV4::GraphicStrokeAlign,
                value: DiscreteValueV4::StrokeAlign { value },
            } => {
                let ElementContentV4::Graphic { graphic } = &mut element.content else {
                    return Err(mismatch());
                };
                graphic.stroke.align = value;
                Ok(())
            }
            _ => Err(mismatch()),
        },
        ChannelTargetV4::Effect { effect_id, .. } => {
            let effect = if let ElementContentV4::EffectLayer { effect } = &mut element.content {
                if effect.id == *effect_id {
                    Some(effect)
                } else {
                    None
                }
            } else {
                element
                    .visual
                    .as_mut()
                    .and_then(|v| v.effects.iter_mut().find(|e| e.id == *effect_id))
            }
            .ok_or_else(mismatch)?;
            let SampledChannelV4::Scalar {
                binding: ScalarBindingV4::Property { parameter },
                physical,
                ..
            } = sample
            else {
                return Err(mismatch());
            };
            effect_property(effect, parameter, physical)
        }
        ChannelTargetV4::Mask { mask_id, .. } => {
            let mask = element
                .visual
                .as_mut()
                .and_then(|v| v.masks.iter_mut().find(|m| m.id == *mask_id))
                .ok_or_else(mismatch)?;
            match sample {
                SampledChannelV4::Scalar {
                    binding: ScalarBindingV4::Property { parameter },
                    physical,
                    ..
                } => mask_property(mask, parameter, physical),
                SampledChannelV4::Scalar {
                    binding:
                        ScalarBindingV4::ColorComponent {
                            parameter: ColorParameterV4::MaskStroke,
                            component,
                        },
                    physical,
                    ..
                } => {
                    color_component(&mut mask.stroke.color, component, physical);
                    Ok(())
                }
                SampledChannelV4::Discrete {
                    parameter: DiscreteParameterV4::MaskStrokeAlign,
                    value: DiscreteValueV4::StrokeAlign { value },
                } => {
                    mask.stroke.align = value;
                    Ok(())
                }
                _ => Err(mismatch()),
            }
        }
        ChannelTargetV4::Track { .. } => Err(mismatch()),
    }
}
