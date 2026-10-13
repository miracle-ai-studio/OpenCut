//! Explicit pinned-caption execution partition. The source graph remains fixed;
//! only its already-sampled caption nodes become internal raster text jobs.
use super::*;
use opencut_contracts::text_frame_v3::CaptionRasterPlanV3;
pub const CLASSIC_TEXT_FRAME_PROFILE_V3: &str = "classic-v31-gpu-normal-caption-frame-v3";
pub const CLASSIC_CAPTION_WRAP_PROFILE_V3: &str = "classic-v31-greedy-whitespace-v1";
#[derive(Clone, Debug)]
pub struct PreparedTextFrameV3 {
    source_graph: CompositionV4,
    execution: PreparedTextFrameV2,
    captions: Vec<CaptionRasterPlanV3>,
}
impl PreparedTextFrameV3 {
    pub fn source_graph(&self) -> &CompositionV4 {
        &self.source_graph
    }
    pub fn execution(&self) -> &PreparedTextFrameV2 {
        &self.execution
    }
    pub fn captions(&self) -> &[CaptionRasterPlanV3] {
        &self.captions
    }
}
pub fn prepare_text_frame_v3(
    graph: &CompositionV4,
) -> Result<PreparedTextFrameV3, CompositionError> {
    let mut execution = graph.clone();
    let mut captions = Vec::new();
    let width = f64::from(graph.format.canvas_size.width);
    let height = f64::from(graph.format.canvas_size.height);
    for layer in &mut execution.layers {
        let SampledContentV4::Caption { text, style } = &layer.content else {
            continue;
        };
        if style.layout != CaptionLayoutProfileV4::PinnedCaptionV4 {
            return Err(unsupported(
                "V3 caption raster requires pinned_caption_v4; legacy captions stay on the legacy renderer",
            ));
        }
        let FontBindingV4::Locked { font_id } = &style.font else {
            return Err(unsupported("V3 captions require an authorized locked font"));
        };
        let font = graph
            .fonts
            .iter()
            .find(|font| &font.id == font_id)
            .ok_or_else(|| invalid("Caption font resource is missing"))?;
        let font_size_px = match style.font_size {
            FontSizeV4::ClassicHeight90 { value } => value * height / 90.0,
            FontSizeV4::CanvasPixels { value } => value,
        };
        if !font_size_px.is_finite() || font_size_px <= 0.0 {
            return Err(invalid("Caption font size must be positive"));
        }
        let line_height = style
            .line_height_px
            .map(f64::from)
            .unwrap_or(font_size_px * 1.2);
        if !line_height.is_finite() || line_height <= 0.0 {
            return Err(invalid("Caption line height must be positive"));
        }
        let max_width = style.max_width_px.map(f64::from).unwrap_or(width * 0.8);
        if max_width <= 0.0 || max_width > width {
            return Err(invalid("Caption width must fit its canvas"));
        }
        captions.push(CaptionRasterPlanV3 {
            element_id: layer.element_id.clone(),
            max_width_px: max_width,
            bottom_margin_px: height * 0.05,
            wrap_profile: CLASSIC_CAPTION_WRAP_PROFILE_V3.into(),
        });
        let clear = ColorV4 {
            encoding: ColorEncodingV4::Srgb,
            red: 0.0,
            green: 0.0,
            blue: 0.0,
            alpha: 0.0,
        };
        layer.content = SampledContentV4::Text {
            text: TextV4 {
                content: text.clone(),
                font: style.font.clone(),
                font_weight: if font.weight == 700 {
                    FontWeightV4::Bold
                } else {
                    FontWeightV4::Normal
                },
                font_style: font.style,
                decoration: TextDecorationV4::None,
                align: style.text_align,
                font_size: style.font_size,
                color: style.color.clone(),
                letter_spacing_px: 0.0,
                line_height_multiplier: line_height / font_size_px,
                background: TextBackgroundV4 {
                    enabled: style.background_color.is_some(),
                    color: style.background_color.clone().unwrap_or(clear),
                    corner_radius_percent: 0.0,
                    padding_x_at_size15: 0.0,
                    padding_y_at_size15: 0.0,
                    offset_x_px: 0.0,
                    offset_y_px: 0.0,
                },
            },
        };
    }
    Ok(PreparedTextFrameV3 {
        source_graph: graph.clone(),
        execution: prepare_text_frame_v2(&execution)?,
        captions,
    })
}
