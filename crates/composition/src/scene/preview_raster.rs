//! A pure raster projection of sampled IR. Source clocks, normalized geometry,
//! resource sizes, graphic-local strokes and authored project data stay exact.
use super::*;

pub fn preview_canvas(source: CanvasSize) -> CanvasSize {
    let ratio = (960.0 / f64::from(source.width)).min(540.0 / f64::from(source.height)).min(1.0);
    if ratio == 1.0 { return source; }
    CanvasSize { width: ((f64::from(source.width) * ratio / 2.0).round() as u32 * 2).max(2),
        height: ((f64::from(source.height) * ratio / 2.0).round() as u32 * 2).max(2) }
}
fn font(size: &mut FontSize, scale: f64) {
    if let FontSize::CanvasPixels { value } = size { *value *= scale; }
}
pub fn project_preview_raster(mut graph: SceneComposition) -> SceneComposition {
    let source = graph.format.canvas_size;
    let target = preview_canvas(source);
    let sx = f64::from(target.width) / f64::from(source.width);
    let sy = f64::from(target.height) / f64::from(source.height);
    let scale = sx.min(sy);
    graph.format.canvas_size = target;
    for layer in &mut graph.layers {
        if let Some(visual) = &mut layer.visual {
            visual.transform.position_x_px *= sx;
            visual.transform.position_y_px *= sy;
            for effect in &mut visual.effects { scale_effect(effect, scale); }
            for mask in &mut visual.masks {
                if let MaskFeather::ClassicPixels { value } = &mut mask.feather { *value *= scale; }
                mask.stroke.width_px *= scale;
                if let LayerMaskShape::Text { mask } = &mut mask.shape {
                    font(&mut mask.font_size, scale);
                    mask.letter_spacing_px *= scale;
                }
            }
        }
        match &mut layer.content {
            SampledContent::Text { text } => {
                // Height90 scales font pixels in the existing text compiler, but
                // its padding uses authored size, so project that padding once.
                if matches!(text.font_size, FontSize::ClassicHeight90 { .. }) {
                    text.background.padding_x_at_size15 *= sx;
                    text.background.padding_y_at_size15 *= sy;
                }
                font(&mut text.font_size, scale);
                text.letter_spacing_px *= scale;
                text.background.offset_x_px *= sx;
                text.background.offset_y_px *= sy;
            }
            SampledContent::Caption { style, .. } => {
                font(&mut style.font_size, scale);
                style.max_width_px = style.max_width_px.map(|v| (f64::from(v) * sx).round().max(1.0) as u32);
                style.line_height_px = style.line_height_px.map(|v| v * sy as f32);
            }
            SampledContent::EffectLayer { effect } => scale_effect(effect, scale),
            // Graphic/sticker/media intrinsic pixels are resource coordinates.
            // Their contain transform already projects them into target pixels.
            _ => {}
        }
    }
    graph
}
fn scale_effect(effect: &mut SceneEffect, scale: f64) {
    if let EffectKind::LegacyGaussianBlur { radius_px } = &mut effect.effect { *radius_px *= scale; }
    // Classic blur intensity is defined relative to output canvas dimensions.
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn target_is_real_bounded_even_raster_without_upscaling() {
        assert_eq!(preview_canvas(CanvasSize {width:1920,height:1080}), CanvasSize {width:960,height:540});
        assert_eq!(preview_canvas(CanvasSize {width:3840,height:2160}), CanvasSize {width:960,height:540});
        assert_eq!(preview_canvas(CanvasSize {width:320,height:180}), CanvasSize {width:320,height:180});
        assert_eq!(preview_canvas(CanvasSize {width:1080,height:1920}), CanvasSize {width:304,height:540});
    }
}
