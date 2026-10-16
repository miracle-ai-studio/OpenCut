//! Pure preparation of an already-sampled free-text IR layer. Font discovery,
//! byte inspection, glyph coverage and raster metrics belong to native/browser
//! ports; this module neither opens resources nor samples a second clock.
use super::*;

#[derive(Clone, Debug, PartialEq)]
pub struct TextRasterPlanV1 {
    pub identity: TextRasterIdentityV1,
    /// The original fixed-revision resource, including byte identity and face.
    /// Native preparation must inspect these bytes against `text.content`.
    pub font: FontResourceV4,
    pub text: ClassicTextRasterV1,
    /// Applied once by the GPU to the full-canvas raster, not by Canvas2D.
    pub opacity: f64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextRasterIdentityV1 {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub element_id: String,
    pub time: Time,
    pub local_time: Time,
    pub width: u32,
    pub height: u32,
}

/// Values correspond to the private browser `text` payload. Public versioned
/// DTOs remain separate and explicitly map the domain enums to wire spellings.
#[derive(Clone, Debug, PartialEq)]
pub struct ClassicTextRasterV1 {
    pub content: String,
    pub font_size_px: f64,
    pub line_height_px: f64,
    pub letter_spacing_px: f64,
    pub align: TextAlignV4,
    pub decoration: TextDecorationV4,
    pub color: [f64; 4],
    pub background: TextRasterBackgroundV1,
    pub transform: TextRasterTransformV1,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextRasterBackgroundV1 {
    pub enabled: bool,
    pub color: [f64; 4],
    pub padding_x_px: f64,
    pub padding_y_px: f64,
    pub offset_x_px: f64,
    pub offset_y_px: f64,
    pub corner_radius_percent: f64,
}

/// Classic text raster order is T · S · R. This is deliberately not a media
/// quad: nonuniform scale and rotation do not commute. The resulting raster
/// must be uploaded through an identity, full-canvas GPU quad.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextRasterTransformV1 {
    pub center_x: f64,
    pub center_y: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub rotation_radians: f64,
}

fn canvas_number(value: f64) -> Result<f64, CompositionError> {
    if value.is_finite() && (value as f32).is_finite() {
        // The browser's existing numerical admission requires finite f32
        // range, but Canvas2D receives the original f64 precision.
        Ok(value)
    } else {
        Err(unsupported(
            "Classic text exceeds finite canvas numeric range",
        ))
    }
}

fn positive(value: f64) -> Result<f64, CompositionError> {
    let value = canvas_number(value)?;
    if value > 0.0 {
        Ok(value)
    } else {
        Err(invalid(
            "Classic text size and line height must be positive",
        ))
    }
}

fn authored_positive(value: f64) -> Result<f64, CompositionError> {
    if value.is_finite() && value > 0.0 {
        Ok(value)
    } else {
        Err(invalid(
            "Sampled text size and line-height multiplier must be positive",
        ))
    }
}

fn rgba(color: &ColorV4) -> Result<[f64; 4], CompositionError> {
    let values = [color.red, color.green, color.blue, color.alpha];
    if color.encoding != ColorEncodingV4::Srgb
        || values
            .iter()
            .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
    {
        return Err(unsupported("Classic text requires normalized sRGB colors"));
    }
    Ok(values)
}

/// Prepare one free-text layer from the fixed graph. The frame assembler keeps
/// the graph's complete layer order and handles media layers separately.
pub fn prepare_text_raster_v1(
    graph: &CompositionV4,
    element_id: &str,
) -> Result<TextRasterPlanV1, CompositionError> {
    reject_legacy_synthesis_fonts(&graph.fonts)?;
    if graph.scope != CompositionScope::Visual
        || graph.format.profile != FormatProfileV4::ClassicGlobal
        || !graph.audio.is_empty()
    {
        return Err(unsupported("Classic text requires a Classic visual graph"));
    }
    let canvas = graph.format.canvas_size;
    if graph.project_id.is_empty()
        || graph.revision.is_empty()
        || graph.sequence_id.is_empty()
        || element_id.is_empty()
        || time(&graph.time)?.ticks() < 0
        || canvas.width == 0
        || canvas.height == 0
    {
        return Err(invalid("Invalid fixed text raster identity"));
    }
    let mut matching = graph
        .layers
        .iter()
        .filter(|layer| layer.element_id == element_id);
    let layer = matching
        .next()
        .ok_or_else(|| invalid("Text layer missing from fixed graph"))?;
    if matching.next().is_some() {
        return Err(invalid("Duplicate text layer identity"));
    }
    time(&layer.local_time)?;
    let SampledContentV4::Text { text } = &layer.content else {
        return Err(unsupported(
            "Text raster requires free text, not captions or other nodes",
        ));
    };
    let visual = layer
        .visual
        .as_ref()
        .ok_or_else(|| invalid("Text layer style missing"))?;
    if visual.profile != VisualProfileV4::ClassicCanvasV31
        || visual.opacity_policy != LayerOpacityPolicyV4::MultiplyTrack
        || visual.blend_mode != BlendModeV4::Normal
        || visual.effects.iter().any(|effect| effect.enabled)
        || visual.masks.iter().any(|mask| mask.enabled)
        || graph.transitions.iter().any(|transition| {
            transition.incoming_element_id == element_id
                || transition.outgoing_element_id == element_id
        })
    {
        return Err(unsupported(
            "Text layer requires unsupported profile, blend, effect, mask or transition",
        ));
    }
    let FontBindingV4::Locked { font_id } = &text.font else {
        return Err(unsupported("Classic text requires a locked font resource"));
    };
    let mut matching_fonts = graph.fonts.iter().filter(|font| &font.id == font_id);
    let font = matching_fonts
        .next()
        .ok_or_else(|| invalid("Locked text font is missing"))?;
    if matching_fonts.next().is_some() {
        return Err(invalid("Duplicate locked text font identity"));
    }
    let weight = match text.font_weight {
        FontWeightV4::Normal => 400,
        FontWeightV4::Bold => 700,
    };
    if font.face_index != 0 || font.weight != weight || font.style != text.font_style {
        return Err(unsupported(
            "Classic text requires a matching static font face without synthesis",
        ));
    }
    let (authored_size, font_size_px) = match text.font_size {
        FontSizeV4::ClassicHeight90 { value } => (
            authored_positive(value)?,
            positive(value * (f64::from(canvas.height) / 90.0))?,
        ),
        FontSizeV4::CanvasPixels { value } => (positive(value)?, positive(value)?),
    };
    // Padding is authored at size 15, not at 15 output pixels. In particular,
    // Classic height scaling must not accidentally scale the padding twice.
    let padding_ratio = authored_size / 15.0;
    let transform = visual.transform;
    Ok(TextRasterPlanV1 {
        identity: TextRasterIdentityV1 {
            project_id: graph.project_id.clone(),
            revision: graph.revision.clone(),
            sequence_id: graph.sequence_id.clone(),
            element_id: element_id.into(),
            time: graph.time.clone(),
            local_time: layer.local_time.clone(),
            width: canvas.width,
            height: canvas.height,
        },
        font: font.clone(),
        text: ClassicTextRasterV1 {
            content: text.content.clone(),
            font_size_px,
            line_height_px: positive(
                font_size_px * authored_positive(text.line_height_multiplier)?,
            )?,
            letter_spacing_px: canvas_number(text.letter_spacing_px)?,
            align: text.align,
            decoration: text.decoration,
            color: rgba(&text.color)?,
            background: TextRasterBackgroundV1 {
                enabled: text.background.enabled,
                color: rgba(&text.background.color)?,
                padding_x_px: canvas_number(text.background.padding_x_at_size15 * padding_ratio)?,
                padding_y_px: canvas_number(text.background.padding_y_at_size15 * padding_ratio)?,
                offset_x_px: canvas_number(text.background.offset_x_px)?,
                offset_y_px: canvas_number(text.background.offset_y_px)?,
                corner_radius_percent: canvas_number(text.background.corner_radius_percent)?,
            },
            transform: TextRasterTransformV1 {
                center_x: canvas_number(f64::from(canvas.width) / 2.0 + transform.position_x_px)?,
                center_y: canvas_number(f64::from(canvas.height) / 2.0 + transform.position_y_px)?,
                scale_x: canvas_number(transform.scale_x)?,
                scale_y: canvas_number(transform.scale_y)?,
                rotation_radians: canvas_number(
                    transform.rotation_degrees * std::f64::consts::PI / 180.0,
                )?,
            },
        },
        opacity: canvas_number(visual.effective_opacity)?,
    })
}

#[cfg(test)]
mod tests;
