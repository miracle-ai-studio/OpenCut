//! Pure V4 visual rules. No I/O or renderer capability claims.
use super::*;

pub(super) fn stroke(value: &StrokeV4) -> Result<(), EditorError> {
    color(&value.color)?;
    nonnegative(value.width_px, "Stroke width")
}

pub(super) fn graphic(value: &GraphicV4) -> Result<(), EditorError> {
    canvas(&value.source_size)?;
    color(&value.fill)?;
    stroke(&value.stroke)?;
    match value.geometry {
        GraphicGeometryV4::Rectangle {
            corner_radius_percent,
        } => bound(corner_radius_percent, 0.0, 100.0, "Graphic corner radius"),
        GraphicGeometryV4::Ellipse => Ok(()),
        GraphicGeometryV4::Polygon {
            sides,
            corner_radius_percent,
        } => {
            bound(sides, 3.0, f64::MAX, "Polygon side count")?;
            bound(corner_radius_percent, 0.0, 100.0, "Graphic corner radius")
        }
        GraphicGeometryV4::Star {
            points,
            depth_percent,
        } => {
            bound(points, 2.0, f64::MAX, "Star point count")?;
            bound(depth_percent, 0.0, 100.0, "Star depth percent")
        }
        GraphicGeometryV4::LegacyRectangle {
            width_px,
            height_px,
        } => {
            positive(width_px, "Legacy shape width")?;
            positive(height_px, "Legacy shape height")
        }
        GraphicGeometryV4::LegacyEllipse {
            radius_x_px,
            radius_y_px,
        } => {
            positive(radius_x_px, "Legacy radius X")?;
            positive(radius_y_px, "Legacy radius Y")
        }
    }
}

pub(super) fn effect(value: &EffectV4, ids: &mut HashSet<String>) -> Result<(), EditorError> {
    unique(ids, &value.id)?;
    match value.effect {
        EffectKindV4::ClassicBlur { intensity } => nonnegative(intensity, "Classic blur intensity"),
        EffectKindV4::LegacyGaussianBlur { radius_px } => {
            bound(radius_px, 0.0, 128.0, "Legacy blur radius")
        }
        EffectKindV4::LegacyColorAdjust {
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

pub(super) fn box_mask(value: &BoxMaskGeometryV4) -> Result<(), EditorError> {
    for x in [value.center_x, value.center_y, value.rotation_degrees] {
        finite(x, "Mask geometry")?;
    }
    positive(value.width, "Mask width")?;
    positive(value.height, "Mask height")?;
    nonnegative(value.scale, "Mask scale")
}

pub(super) fn mask(
    value: &MaskV4,
    fonts: &HashMap<&str, &FontResourceV4>,
    ids: &mut HashSet<String>,
) -> Result<(), EditorError> {
    unique(ids, &value.id)?;
    stroke(&value.stroke)?;
    match value.feather {
        MaskFeatherV4::ClassicPixels { value } => nonnegative(value, "Mask feather pixels")?,
        MaskFeatherV4::LegacyCanvasFraction { value } => {
            bound(value, 0.0, 0.5, "Legacy mask feather")?
        }
    }
    match &value.shape {
        MaskShapeV4::Split {
            center_x,
            center_y,
            rotation_degrees,
        } => {
            for x in [*center_x, *center_y, *rotation_degrees] {
                finite(x, "Split mask geometry")?;
            }
        }
        MaskShapeV4::CinematicBars { geometry }
        | MaskShapeV4::Rectangle { geometry }
        | MaskShapeV4::Ellipse { geometry }
        | MaskShapeV4::Heart { geometry }
        | MaskShapeV4::Diamond { geometry }
        | MaskShapeV4::Star { geometry } => box_mask(geometry)?,
        MaskShapeV4::Text { mask } => {
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
        MaskShapeV4::Freeform { path } => {
            if path.points.len() < if path.closed { 3 } else { 2 } {
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
        MaskShapeV4::LegacyRectangle {
            x,
            y,
            width,
            height,
        }
        | MaskShapeV4::LegacyEllipse {
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
    value: &VisualStyleV4,
    fonts: &HashMap<&str, &FontResourceV4>,
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
