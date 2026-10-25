//! Pure V4 visual rules. No I/O or renderer capability claims.
use super::*;

pub(super) fn stroke(value: &Stroke) -> Result<(), EditorError> {
    color(&value.color)?;
    nonnegative(value.width_px, "Stroke width")
}

pub(super) fn graphic(value: &Graphic) -> Result<(), EditorError> {
    canvas(&value.source_size)?;
    color(&value.fill)?;
    stroke(&value.stroke)?;
    match value.geometry {
        GraphicGeometry::Rectangle {
            corner_radius_percent,
        } => bound(corner_radius_percent, 0.0, 100.0, "Graphic corner radius"),
        GraphicGeometry::Ellipse => Ok(()),
        GraphicGeometry::Polygon {
            sides,
            corner_radius_percent,
        } => {
            bound(sides, 3.0, f64::MAX, "Polygon side count")?;
            bound(corner_radius_percent, 0.0, 100.0, "Graphic corner radius")
        }
        GraphicGeometry::Star {
            points,
            depth_percent,
        } => {
            bound(points, 2.0, f64::MAX, "Star point count")?;
            bound(depth_percent, 0.0, 100.0, "Star depth percent")
        }
        GraphicGeometry::LegacyRectangle {
            width_px,
            height_px,
        } => {
            positive(width_px, "Legacy shape width")?;
            positive(height_px, "Legacy shape height")
        }
        GraphicGeometry::LegacyEllipse {
            radius_x_px,
            radius_y_px,
        } => {
            positive(radius_x_px, "Legacy radius X")?;
            positive(radius_y_px, "Legacy radius Y")
        }
    }
}

pub(super) fn effect(value: &SceneEffect, ids: &mut HashSet<String>) -> Result<(), EditorError> {
    unique(ids, &value.id)?;
    match value.effect {
        EffectKind::ClassicBlur { intensity } => nonnegative(intensity, "Classic blur intensity"),
        EffectKind::LegacyGaussianBlur { radius_px } => {
            bound(radius_px, 0.0, 128.0, "Legacy blur radius")
        }
        EffectKind::LegacyColorAdjust {
            exposure_ev,
            contrast,
            saturation,
        } => {
            bound(exposure_ev, -8.0, 8.0, "Legacy exposure")?;
            bound(contrast, 0.0, 4.0, "Legacy contrast")?;
            bound(saturation, 0.0, 4.0, "Legacy saturation")
        }
    }
}

pub(super) fn box_mask(value: &BoxMaskGeometry) -> Result<(), EditorError> {
    for x in [value.center_x, value.center_y, value.rotation_degrees] {
        finite(x, "Mask geometry")?;
    }
    positive(value.width, "Mask width")?;
    positive(value.height, "Mask height")?;
    nonnegative(value.scale, "Mask scale")
}

pub(super) fn mask(
    value: &Mask,
    fonts: &HashMap<&str, &FontResource>,
    ids: &mut HashSet<String>,
) -> Result<(), EditorError> {
    unique(ids, &value.id)?;
    stroke(&value.stroke)?;
    match value.feather {
        MaskFeather::ClassicPixels { value } => nonnegative(value, "Mask feather pixels")?,
        MaskFeather::LegacyCanvasFraction { value } => {
            bound(value, 0.0, 0.5, "Legacy mask feather")?
        }
    }
    match &value.shape {
        LayerMaskShape::Split {
            center_x,
            center_y,
            rotation_degrees,
        } => {
            for x in [*center_x, *center_y, *rotation_degrees] {
                finite(x, "Split mask geometry")?;
            }
        }
        LayerMaskShape::CinematicBars { geometry }
        | LayerMaskShape::Rectangle { geometry }
        | LayerMaskShape::Ellipse { geometry }
        | LayerMaskShape::Heart { geometry }
        | LayerMaskShape::Diamond { geometry }
        | LayerMaskShape::Star { geometry } => box_mask(geometry)?,
        LayerMaskShape::Text { mask } => {
            font_request(&mask.font, mask.font_weight, mask.font_style, fonts)?;
            font_size(&mask.font_size)?;
            positive(mask.line_height_multiplier, "Mask line height")?;
            for x in [
                mask.letter_spacing_px,
                mask.center_x,
                mask.center_y,
                mask.rotation_degrees,
            ] {
                finite(x, "Text mask geometry")?;
            }
            nonnegative(mask.scale, "Text mask scale")?;
        }
        LayerMaskShape::Freeform { path } => {
            // Classic preserves open pen drafts, including zero or one point.
            // They are inactive until closed; never drop their authored geometry.
            if path.closed && path.points.len() < 3 {
                return Err(invalid("Freeform mask lacks enough path points"));
            }
            for x in [path.center_x, path.center_y, path.rotation_degrees] {
                finite(x, "Freeform mask geometry")?;
            }
            nonnegative(path.scale, "Freeform mask scale")?;
            for p in &path.points {
                unique(ids, &p.id)?;
                for x in [p.x, p.y, p.in_x, p.in_y, p.out_x, p.out_y] {
                    finite(x, "Freeform point or handle")?;
                }
            }
        }
        LayerMaskShape::LegacyRectangle {
            x,
            y,
            width,
            height,
        }
        | LayerMaskShape::LegacyEllipse {
            x,
            y,
            width,
            height,
        } => {
            bound(*x, 0.0, 1.0, "Legacy mask X")?;
            bound(*y, 0.0, 1.0, "Legacy mask Y")?;
            positive(*width, "Legacy mask width")?;
            positive(*height, "Legacy mask height")?;
            if x + width > 1.0 || y + height > 1.0 {
                return Err(invalid("Legacy mask exceeds canvas"));
            }
        }
    }
    Ok(())
}

pub(super) fn visual(
    value: &VisualStyle,
    fonts: &HashMap<&str, &FontResource>,
    ids: &mut HashSet<String>,
) -> Result<(), EditorError> {
    for x in [
        value.transform.position_x_px,
        value.transform.position_y_px,
        value.transform.scale_x,
        value.transform.scale_y,
        value.transform.rotation_degrees,
    ] {
        finite(x, "Visual transform")?;
    }
    bound(value.opacity, 0.0, 1.0, "Opacity")?;
    for node in &value.effects {
        effect(node, ids)?;
    }
    for node in &value.masks {
        mask(node, fonts, ids)?;
    }
    Ok(())
}
