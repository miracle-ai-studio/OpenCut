//! Shared-registry physical projection for static authored fields.
use super::properties::{box_geometry, effect_property, element_property, mask_property};
use super::*;
pub(super) fn project_static(
    element: &mut TimelineElementV4,
    scope: CompositionScope,
) -> Result<(), CompositionError> {
    let mut fields = vec![];
    if let Some(v) = &element.visual {
        fields.extend([
            (ScalarParameterV4::PositionXPx, v.transform.position_x_px),
            (ScalarParameterV4::PositionYPx, v.transform.position_y_px),
            (ScalarParameterV4::ScaleX, v.transform.scale_x),
            (ScalarParameterV4::ScaleY, v.transform.scale_y),
            (
                ScalarParameterV4::RotationDegrees,
                v.transform.rotation_degrees,
            ),
            (ScalarParameterV4::Opacity, v.opacity),
        ]);
    }
    match &element.content {
        ElementContentV4::Video { audio, .. } | ElementContentV4::Audio { audio, .. }
            if scope == CompositionScope::AudioVisual =>
        {
            let (p, v) = match audio.gain {
                GainV4::LinearLegacy { value } => (ScalarParameterV4::GainLinearLegacy, value),
                GainV4::Decibels { value } => (ScalarParameterV4::GainDecibels, value),
            };
            fields.push((p, v));
        }
        ElementContentV4::Text { text } => {
            let (p, v) = match text.font_size {
                FontSizeV4::ClassicHeight90 { value } => {
                    (ScalarParameterV4::FontSizeClassicHeight90, value)
                }
                FontSizeV4::CanvasPixels { value } => {
                    (ScalarParameterV4::FontSizeCanvasPixels, value)
                }
            };
            fields.push((p, v));
            fields.extend([
                (ScalarParameterV4::LetterSpacingPx, text.letter_spacing_px),
                (
                    ScalarParameterV4::LineHeightMultiplier,
                    text.line_height_multiplier,
                ),
                (
                    ScalarParameterV4::TextBackgroundPaddingXAtSize15,
                    text.background.padding_x_at_size15,
                ),
                (
                    ScalarParameterV4::TextBackgroundPaddingYAtSize15,
                    text.background.padding_y_at_size15,
                ),
                (
                    ScalarParameterV4::TextBackgroundOffsetXPx,
                    text.background.offset_x_px,
                ),
                (
                    ScalarParameterV4::TextBackgroundOffsetYPx,
                    text.background.offset_y_px,
                ),
                (
                    ScalarParameterV4::TextBackgroundCornerRadiusPercent,
                    text.background.corner_radius_percent,
                ),
            ]);
        }
        ElementContentV4::Graphic { graphic } => {
            fields.push((
                ScalarParameterV4::GraphicStrokeWidthPx,
                graphic.stroke.width_px,
            ));
            match graphic.geometry {
                GraphicGeometryV4::Rectangle {
                    corner_radius_percent,
                } => fields.push((
                    ScalarParameterV4::GraphicCornerRadiusPercent,
                    corner_radius_percent,
                )),
                GraphicGeometryV4::Polygon {
                    sides,
                    corner_radius_percent,
                } => fields.extend([
                    (ScalarParameterV4::GraphicSides, sides),
                    (
                        ScalarParameterV4::GraphicCornerRadiusPercent,
                        corner_radius_percent,
                    ),
                ]),
                GraphicGeometryV4::Star {
                    points,
                    depth_percent,
                } => fields.extend([
                    (ScalarParameterV4::GraphicPoints, points),
                    (ScalarParameterV4::GraphicDepthPercent, depth_percent),
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
    if let ElementContentV4::EffectLayer { effect } = &mut element.content {
        project_effect(effect)?;
    }
    Ok(())
}
fn project_effect(effect: &mut EffectV4) -> Result<(), CompositionError> {
    if !effect.enabled {
        return Ok(());
    }
    let fields = match effect.effect {
        EffectKindV4::ClassicBlur { intensity } => {
            vec![(ScalarParameterV4::ClassicBlurIntensity, intensity)]
        }
        EffectKindV4::LegacyGaussianBlur { radius_px } => {
            vec![(ScalarParameterV4::LegacyBlurRadiusPx, radius_px)]
        }
        EffectKindV4::LegacyColorAdjust {
            exposure_ev,
            contrast,
            saturation,
        } => vec![
            (ScalarParameterV4::LegacyExposureEv, exposure_ev),
            (ScalarParameterV4::LegacyContrast, contrast),
            (ScalarParameterV4::LegacySaturation, saturation),
        ],
    };
    for (p, v) in fields {
        effect_property(effect, p, property(p, v)?)?;
    }
    Ok(())
}
fn project_mask(mask: &mut MaskV4) -> Result<(), CompositionError> {
    if !mask.enabled {
        return Ok(());
    }
    let mut fields = vec![(ScalarParameterV4::MaskStrokeWidthPx, mask.stroke.width_px)];
    fields.push(match mask.feather {
        MaskFeatherV4::ClassicPixels { value } => {
            (ScalarParameterV4::MaskFeatherClassicPixels, value)
        }
        MaskFeatherV4::LegacyCanvasFraction { value } => {
            (ScalarParameterV4::MaskFeatherLegacyCanvasFraction, value)
        }
    });
    if let Some(g) = box_geometry(&mut mask.shape) {
        fields.extend([
            (ScalarParameterV4::MaskCenterX, g.center_x),
            (ScalarParameterV4::MaskCenterY, g.center_y),
            (ScalarParameterV4::MaskWidth, g.width),
            (ScalarParameterV4::MaskHeight, g.height),
            (ScalarParameterV4::MaskRotationDegrees, g.rotation_degrees),
            (ScalarParameterV4::MaskScale, g.scale),
        ]);
    } else {
        match &mask.shape {
            MaskShapeV4::Split {
                center_x,
                center_y,
                rotation_degrees,
            } => fields.extend([
                (ScalarParameterV4::MaskCenterX, *center_x),
                (ScalarParameterV4::MaskCenterY, *center_y),
                (ScalarParameterV4::MaskRotationDegrees, *rotation_degrees),
            ]),
            MaskShapeV4::Text { mask } => {
                fields.extend([
                    (ScalarParameterV4::MaskCenterX, mask.center_x),
                    (ScalarParameterV4::MaskCenterY, mask.center_y),
                    (
                        ScalarParameterV4::MaskRotationDegrees,
                        mask.rotation_degrees,
                    ),
                    (ScalarParameterV4::MaskScale, mask.scale),
                    (
                        ScalarParameterV4::MaskLetterSpacingPx,
                        mask.letter_spacing_px,
                    ),
                    (
                        ScalarParameterV4::MaskLineHeightMultiplier,
                        mask.line_height_multiplier,
                    ),
                ]);
                fields.push(match mask.font_size {
                    FontSizeV4::ClassicHeight90 { value } => {
                        (ScalarParameterV4::MaskFontSizeClassicHeight90, value)
                    }
                    FontSizeV4::CanvasPixels { value } => {
                        (ScalarParameterV4::MaskFontSizeCanvasPixels, value)
                    }
                });
            }
            MaskShapeV4::Freeform { path } => fields.extend([
                (ScalarParameterV4::MaskCenterX, path.center_x),
                (ScalarParameterV4::MaskCenterY, path.center_y),
                (
                    ScalarParameterV4::MaskRotationDegrees,
                    path.rotation_degrees,
                ),
                (ScalarParameterV4::MaskScale, path.scale),
            ]),
            _ => {}
        }
    }
    for (p, v) in fields {
        mask_property(mask, p, property(p, v)?)?;
    }
    Ok(())
}
