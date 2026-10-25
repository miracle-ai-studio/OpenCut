//! Shared-registry physical projection for static authored fields.
use super::properties::{box_geometry, effect_property, element_property, mask_property};
use super::*;
pub(super) fn project_static(
    element: &mut TimelineElement,
    scope: CompositionScope,
) -> Result<(), CompositionError> {
    let mut fields = vec![];
    if let Some(v) = &element.visual {
        fields.extend([
            (ScalarParameter::PositionXPx, v.transform.position_x_px),
            (ScalarParameter::PositionYPx, v.transform.position_y_px),
            (ScalarParameter::ScaleX, v.transform.scale_x),
            (ScalarParameter::ScaleY, v.transform.scale_y),
            (
                ScalarParameter::RotationDegrees,
                v.transform.rotation_degrees,
            ),
            (ScalarParameter::Opacity, v.opacity),
        ]);
    }
    match &element.content {
        ElementContent::Video { audio, .. } | ElementContent::Audio { audio, .. }
            if scope == CompositionScope::AudioVisual =>
        {
            let (p, v) = match audio.gain {
                Gain::LinearLegacy { value } => (ScalarParameter::GainLinearLegacy, value),
                Gain::Decibels { value } => (ScalarParameter::GainDecibels, value),
            };
            fields.push((p, v));
        }
        ElementContent::Text { text } => {
            let (p, v) = match text.font_size {
                FontSize::ClassicHeight90 { value } => {
                    (ScalarParameter::FontSizeClassicHeight90, value)
                }
                FontSize::CanvasPixels { value } => (ScalarParameter::FontSizeCanvasPixels, value),
            };
            fields.push((p, v));
            fields.extend([
                (ScalarParameter::LetterSpacingPx, text.letter_spacing_px),
                (
                    ScalarParameter::LineHeightMultiplier,
                    text.line_height_multiplier,
                ),
                (
                    ScalarParameter::TextBackgroundPaddingXAtSize15,
                    text.background.padding_x_at_size15,
                ),
                (
                    ScalarParameter::TextBackgroundPaddingYAtSize15,
                    text.background.padding_y_at_size15,
                ),
                (
                    ScalarParameter::TextBackgroundOffsetXPx,
                    text.background.offset_x_px,
                ),
                (
                    ScalarParameter::TextBackgroundOffsetYPx,
                    text.background.offset_y_px,
                ),
                (
                    ScalarParameter::TextBackgroundCornerRadiusPercent,
                    text.background.corner_radius_percent,
                ),
            ]);
        }
        ElementContent::Graphic { graphic } => {
            fields.push((
                ScalarParameter::GraphicStrokeWidthPx,
                graphic.stroke.width_px,
            ));
            match graphic.geometry {
                GraphicGeometry::Rectangle {
                    corner_radius_percent,
                } => fields.push((
                    ScalarParameter::GraphicCornerRadiusPercent,
                    corner_radius_percent,
                )),
                GraphicGeometry::Polygon {
                    sides,
                    corner_radius_percent,
                } => fields.extend([
                    (ScalarParameter::GraphicSides, sides),
                    (
                        ScalarParameter::GraphicCornerRadiusPercent,
                        corner_radius_percent,
                    ),
                ]),
                GraphicGeometry::Star {
                    points,
                    depth_percent,
                } => fields.extend([
                    (ScalarParameter::GraphicPoints, points),
                    (ScalarParameter::GraphicDepthPercent, depth_percent),
                ]),
                _ => {}
            }
        }
        _ => {}
    }
    for (p, v) in fields {
        element_property(element, p, property(p, v)?)?;
    }
    if let Some(v) = &mut element.visual {
        for e in &mut v.effects {
            project_effect(e)?;
        }
        for m in &mut v.masks {
            project_mask(m)?;
        }
    }
    if let ElementContent::EffectLayer { effect } = &mut element.content {
        project_effect(effect)?;
    }
    Ok(())
}
fn project_effect(effect: &mut SceneEffect) -> Result<(), CompositionError> {
    if !effect.enabled {
        return Ok(());
    }
    let fields = match effect.effect {
        EffectKind::ClassicBlur { intensity } => {
            vec![(ScalarParameter::ClassicBlurIntensity, intensity)]
        }
        EffectKind::LegacyGaussianBlur { radius_px } => {
            vec![(ScalarParameter::LegacyBlurRadiusPx, radius_px)]
        }
        EffectKind::LegacyColorAdjust {
            exposure_ev,
            contrast,
            saturation,
        } => vec![
            (ScalarParameter::LegacyExposureEv, exposure_ev),
            (ScalarParameter::LegacyContrast, contrast),
            (ScalarParameter::LegacySaturation, saturation),
        ],
    };
    for (p, v) in fields {
        effect_property(effect, p, property(p, v)?)?;
    }
    Ok(())
}
fn project_mask(mask: &mut Mask) -> Result<(), CompositionError> {
    if !mask.enabled {
        return Ok(());
    }
    let mut fields = vec![(ScalarParameter::MaskStrokeWidthPx, mask.stroke.width_px)];
    fields.push(match mask.feather {
        MaskFeather::ClassicPixels { value } => (ScalarParameter::MaskFeatherClassicPixels, value),
        MaskFeather::LegacyCanvasFraction { value } => {
            (ScalarParameter::MaskFeatherLegacyCanvasFraction, value)
        }
    });
    if let Some(g) = box_geometry(&mut mask.shape) {
        fields.extend([
            (ScalarParameter::MaskCenterX, g.center_x),
            (ScalarParameter::MaskCenterY, g.center_y),
            (ScalarParameter::MaskWidth, g.width),
            (ScalarParameter::MaskHeight, g.height),
            (ScalarParameter::MaskRotationDegrees, g.rotation_degrees),
            (ScalarParameter::MaskScale, g.scale),
        ]);
    } else {
        match &mask.shape {
            LayerMaskShape::Split {
                center_x,
                center_y,
                rotation_degrees,
            } => fields.extend([
                (ScalarParameter::MaskCenterX, *center_x),
                (ScalarParameter::MaskCenterY, *center_y),
                (ScalarParameter::MaskRotationDegrees, *rotation_degrees),
            ]),
            LayerMaskShape::Text { mask } => {
                fields.extend([
                    (ScalarParameter::MaskCenterX, mask.center_x),
                    (ScalarParameter::MaskCenterY, mask.center_y),
                    (ScalarParameter::MaskRotationDegrees, mask.rotation_degrees),
                    (ScalarParameter::MaskScale, mask.scale),
                    (ScalarParameter::MaskLetterSpacingPx, mask.letter_spacing_px),
                    (
                        ScalarParameter::MaskLineHeightMultiplier,
                        mask.line_height_multiplier,
                    ),
                ]);
                fields.push(match mask.font_size {
                    FontSize::ClassicHeight90 { value } => {
                        (ScalarParameter::MaskFontSizeClassicHeight90, value)
                    }
                    FontSize::CanvasPixels { value } => {
                        (ScalarParameter::MaskFontSizeCanvasPixels, value)
                    }
                });
            }
            LayerMaskShape::Freeform { path } => fields.extend([
                (ScalarParameter::MaskCenterX, path.center_x),
                (ScalarParameter::MaskCenterY, path.center_y),
                (ScalarParameter::MaskRotationDegrees, path.rotation_degrees),
                (ScalarParameter::MaskScale, path.scale),
            ]),
            _ => {}
        }
    }
    for (p, v) in fields {
        mask_property(mask, p, property(p, v)?)?;
    }
    Ok(())
}
