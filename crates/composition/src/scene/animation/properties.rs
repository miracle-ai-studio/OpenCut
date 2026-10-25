//! Exhaustive typed field setters; no sampling or document authority.
use super::*;
pub(super) fn mismatch() -> CompositionError {
    unsupported("Animation binding has no matching V4 execution field")
}
pub(super) fn set_gain(
    gain: &mut Gain,
    parameter: ScalarParameter,
    value: f64,
) -> Result<(), CompositionError> {
    match (gain, parameter) {
        (Gain::LinearLegacy { value: output }, ScalarParameter::GainLinearLegacy)
        | (Gain::Decibels { value: output }, ScalarParameter::GainDecibels) => {
            *output = value;
            Ok(())
        }
        _ => Err(mismatch()),
    }
}
pub(super) fn font_size(
    size: &mut FontSize,
    classic: bool,
    value: f64,
) -> Result<(), CompositionError> {
    match (size, classic) {
        (FontSize::ClassicHeight90 { value: to }, true)
        | (FontSize::CanvasPixels { value: to }, false) => {
            *to = value;
            Ok(())
        }
        _ => Err(mismatch()),
    }
}
pub(super) fn text_property(
    text: &mut Text,
    p: ScalarParameter,
    v: f64,
) -> Result<(), CompositionError> {
    use ScalarParameter::*;
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
    graphic: &mut Graphic,
    p: ScalarParameter,
    v: f64,
) -> Result<(), CompositionError> {
    use ScalarParameter::*;
    match (p, &mut graphic.geometry) {
        (GraphicStrokeWidthPx, _) => graphic.stroke.width_px = v,
        (
            GraphicCornerRadiusPercent,
            GraphicGeometry::Rectangle {
                corner_radius_percent,
            }
            | GraphicGeometry::Polygon {
                corner_radius_percent,
                ..
            },
        ) => *corner_radius_percent = v,
        (GraphicSides, GraphicGeometry::Polygon { sides, .. }) => *sides = v,
        (GraphicPoints, GraphicGeometry::Star { points, .. }) => *points = v,
        (GraphicDepthPercent, GraphicGeometry::Star { depth_percent, .. }) => *depth_percent = v,
        _ => return Err(mismatch()),
    }
    Ok(())
}
pub(super) fn effect_property(
    effect: &mut SceneEffect,
    p: ScalarParameter,
    v: f64,
) -> Result<(), CompositionError> {
    use ScalarParameter::*;
    match (p, &mut effect.effect) {
        (ClassicBlurIntensity, EffectKind::ClassicBlur { intensity }) => *intensity = v,
        (LegacyBlurRadiusPx, EffectKind::LegacyGaussianBlur { radius_px }) => *radius_px = v,
        (LegacyExposureEv, EffectKind::LegacyColorAdjust { exposure_ev, .. }) => *exposure_ev = v,
        (LegacyContrast, EffectKind::LegacyColorAdjust { contrast, .. }) => *contrast = v,
        (LegacySaturation, EffectKind::LegacyColorAdjust { saturation, .. }) => *saturation = v,
        _ => return Err(mismatch()),
    }
    Ok(())
}
pub(super) fn box_geometry(shape: &mut LayerMaskShape) -> Option<&mut BoxMaskGeometry> {
    match shape {
        LayerMaskShape::CinematicBars { geometry }
        | LayerMaskShape::Rectangle { geometry }
        | LayerMaskShape::Ellipse { geometry }
        | LayerMaskShape::Heart { geometry }
        | LayerMaskShape::Diamond { geometry }
        | LayerMaskShape::Star { geometry } => Some(geometry),
        _ => None,
    }
}
pub(super) fn mask_property(
    mask: &mut Mask,
    p: ScalarParameter,
    v: f64,
) -> Result<(), CompositionError> {
    use ScalarParameter::*;
    match p {
        MaskFeatherClassicPixels => match &mut mask.feather {
            MaskFeather::ClassicPixels { value } => *value = v,
            _ => return Err(mismatch()),
        },
        MaskFeatherLegacyCanvasFraction => match &mut mask.feather {
            MaskFeather::LegacyCanvasFraction { value } => *value = v,
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
                    LayerMaskShape::Split {
                        center_x,
                        center_y,
                        rotation_degrees,
                    } => match p {
                        MaskCenterX => *center_x = v,
                        MaskCenterY => *center_y = v,
                        MaskRotationDegrees => *rotation_degrees = v,
                        _ => return Err(mismatch()),
                    },
                    LayerMaskShape::Text { mask } => match p {
                        MaskCenterX => mask.center_x = v,
                        MaskCenterY => mask.center_y = v,
                        MaskRotationDegrees => mask.rotation_degrees = v,
                        MaskScale => mask.scale = v,
                        _ => unreachable!(),
                    },
                    LayerMaskShape::Freeform { path } => match p {
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
            let LayerMaskShape::Text { mask } = &mut mask.shape else {
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
    element: &mut TimelineElement,
    p: ScalarParameter,
    v: f64,
) -> Result<(), CompositionError> {
    use ScalarParameter::*;
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
                ElementContent::Video { audio, .. } | ElementContent::Audio { audio, .. } => audio,
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
            let ElementContent::Text { text } = &mut element.content else {
                return Err(mismatch());
            };
            return text_property(text, p, v);
        }
        GraphicStrokeWidthPx
        | GraphicCornerRadiusPercent
        | GraphicSides
        | GraphicPoints
        | GraphicDepthPercent => {
            let ElementContent::Graphic { graphic } = &mut element.content else {
                return Err(mismatch());
            };
            return graphic_property(graphic, p, v);
        }
        _ => return Err(mismatch()),
    }
    Ok(())
}

fn color_component(color: &mut Color, component: ColorComponent, v: f64) {
    if component != ColorComponent::Alpha && color.encoding == ColorEncoding::Srgb {
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
        color.encoding = ColorEncoding::LinearSrgb;
    }
    match component {
        ColorComponent::Red => color.red = v,
        ColorComponent::Green => color.green = v,
        ColorComponent::Blue => color.blue = v,
        ColorComponent::Alpha => color.alpha = v,
    }
}
fn element_color(
    element: &mut TimelineElement,
    p: ColorParameter,
) -> Result<&mut Color, CompositionError> {
    match (p, &mut element.content) {
        (ColorParameter::TextColor, ElementContent::Text { text }) => Ok(&mut text.color),
        (ColorParameter::TextBackgroundColor, ElementContent::Text { text }) => {
            Ok(&mut text.background.color)
        }
        (ColorParameter::GraphicFill, ElementContent::Graphic { graphic }) => Ok(&mut graphic.fill),
        (ColorParameter::GraphicStroke, ElementContent::Graphic { graphic }) => {
            Ok(&mut graphic.stroke.color)
        }
        _ => Err(mismatch()),
    }
}
pub(super) fn apply(
    element: &mut TimelineElement,
    target: &ChannelTarget,
    sample: SampledChannel,
) -> Result<(), CompositionError> {
    match target {
        ChannelTarget::Element { .. } => match sample {
            SampledChannel::Scalar {
                binding: ScalarBinding::Property { parameter },
                physical,
                ..
            } => element_property(element, parameter, physical),
            SampledChannel::Scalar {
                binding:
                    ScalarBinding::ColorComponent {
                        parameter,
                        component,
                    },
                physical,
                ..
            } => {
                color_component(element_color(element, parameter)?, component, physical);
                Ok(())
            }
            SampledChannel::Discrete {
                parameter: DiscreteParameter::GraphicStrokeAlign,
                value: DiscreteValue::StrokeAlign { value },
            } => {
                let ElementContent::Graphic { graphic } = &mut element.content else {
                    return Err(mismatch());
                };
                graphic.stroke.align = value;
                Ok(())
            }
            _ => Err(mismatch()),
        },
        ChannelTarget::Effect { effect_id, .. } => {
            let effect = if let ElementContent::EffectLayer { effect } = &mut element.content {
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
            let SampledChannel::Scalar {
                binding: ScalarBinding::Property { parameter },
                physical,
                ..
            } = sample
            else {
                return Err(mismatch());
            };
            effect_property(effect, parameter, physical)
        }
        ChannelTarget::Mask { mask_id, .. } => {
            let mask = element
                .visual
                .as_mut()
                .and_then(|v| v.masks.iter_mut().find(|m| m.id == *mask_id))
                .ok_or_else(mismatch)?;
            match sample {
                SampledChannel::Scalar {
                    binding: ScalarBinding::Property { parameter },
                    physical,
                    ..
                } => mask_property(mask, parameter, physical),
                SampledChannel::Scalar {
                    binding:
                        ScalarBinding::ColorComponent {
                            parameter: ColorParameter::MaskStroke,
                            component,
                        },
                    physical,
                    ..
                } => {
                    color_component(&mut mask.stroke.color, component, physical);
                    Ok(())
                }
                SampledChannel::Discrete {
                    parameter: DiscreteParameter::MaskStrokeAlign,
                    value: DiscreteValue::StrokeAlign { value },
                } => {
                    mask.stroke.align = value;
                    Ok(())
                }
                _ => Err(mismatch()),
            }
        }
        ChannelTarget::Track { .. } => Err(mismatch()),
    }
}
