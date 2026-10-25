//! Shared parameter/target matrix. Ownership resolution remains in channels.
use super::*;

#[derive(Clone, Copy, Debug)]
pub enum ParameterTarget<'a> {
    Element(&'a TimelineElement),
    Effect(&'a SceneEffect),
    Mask(&'a Mask),
    Track(&'a SceneTrack),
}

fn element_gain(element: &TimelineElement) -> Option<Gain> {
    match &element.content {
        ElementContent::Video { audio, .. } | ElementContent::Audio { audio, .. } => {
            Some(audio.gain)
        }
        _ => None,
    }
}
fn box_shape(shape: &LayerMaskShape) -> bool {
    match shape {
        LayerMaskShape::CinematicBars { .. }
        | LayerMaskShape::Rectangle { .. }
        | LayerMaskShape::Ellipse { .. }
        | LayerMaskShape::Heart { .. }
        | LayerMaskShape::Diamond { .. }
        | LayerMaskShape::Star { .. } => true,
        LayerMaskShape::Split { .. }
        | LayerMaskShape::Text { .. }
        | LayerMaskShape::Freeform { .. }
        | LayerMaskShape::LegacyRectangle { .. }
        | LayerMaskShape::LegacyEllipse { .. } => false,
    }
}
fn applies(requirement: ParameterApplicability, target: &ParameterTarget<'_>) -> bool {
    use ParameterApplicability as A;
    use ParameterTarget as T;
    match requirement {
        A::VisualElement => matches!(target, T::Element(e) if e.visual.is_some()),
        A::VisualElementOrTrack => {
            matches!(target, T::Element(e) if e.visual.is_some())
                || matches!(target, T::Track(t) if t.kind != SceneTrackKind::Audio)
        }
        A::LinearGain | A::DecibelGain => {
            let gain = match target {
                T::Element(e) => element_gain(e),
                T::Track(t) if matches!(t.kind, SceneTrackKind::Video | SceneTrackKind::Audio) => {
                    Some(t.gain)
                }
                T::Track(_) | T::Effect(_) | T::Mask(_) => None,
            };
            matches!(
                (requirement, gain),
                (A::LinearGain, Some(Gain::LinearLegacy { .. }))
                    | (A::DecibelGain, Some(Gain::Decibels { .. }))
            )
        }
        A::ClassicTextFontSize => matches!(target, T::Element(e)
            if matches!(&e.content, ElementContent::Text { text } if matches!(text.font_size, FontSize::ClassicHeight90 { .. }))),
        A::PixelTextFontSize => matches!(target, T::Element(e)
            if matches!(&e.content, ElementContent::Text { text } if matches!(text.font_size, FontSize::CanvasPixels { .. }))),
        A::Text => {
            matches!(target, T::Element(e) if matches!(e.content, ElementContent::Text { .. }))
        }
        A::Graphic => {
            matches!(target, T::Element(e) if matches!(e.content, ElementContent::Graphic { .. }))
        }
        A::GraphicRectangleOrPolygon => matches!(target, T::Element(e)
            if matches!(&e.content, ElementContent::Graphic { graphic } if matches!(graphic.geometry, GraphicGeometry::Rectangle { .. } | GraphicGeometry::Polygon { .. }))),
        A::GraphicPolygon => matches!(target, T::Element(e)
            if matches!(&e.content, ElementContent::Graphic { graphic } if matches!(graphic.geometry, GraphicGeometry::Polygon { .. }))),
        A::GraphicStar => matches!(target, T::Element(e)
            if matches!(&e.content, ElementContent::Graphic { graphic } if matches!(graphic.geometry, GraphicGeometry::Star { .. }))),
        A::ClassicBlurEffect => {
            matches!(target, T::Effect(e) if matches!(e.effect, EffectKind::ClassicBlur { .. }))
        }
        A::LegacyBlurEffect => {
            matches!(target, T::Effect(e) if matches!(e.effect, EffectKind::LegacyGaussianBlur { .. }))
        }
        A::LegacyColorEffect => {
            matches!(target, T::Effect(e) if matches!(e.effect, EffectKind::LegacyColorAdjust { .. }))
        }
        A::ClassicMaskGeometry => matches!(target, T::Mask(m)
            if !matches!(m.shape, LayerMaskShape::LegacyRectangle { .. } | LayerMaskShape::LegacyEllipse { .. })),
        A::BoxMask => matches!(target, T::Mask(m) if box_shape(&m.shape)),
        A::ScalableMask => matches!(target, T::Mask(m)
            if box_shape(&m.shape) || matches!(m.shape, LayerMaskShape::Text { .. } | LayerMaskShape::Freeform { .. })),
        A::ClassicMaskFeather => {
            matches!(target, T::Mask(m) if matches!(m.feather, MaskFeather::ClassicPixels { .. }))
        }
        A::LegacyMaskFeather => {
            matches!(target, T::Mask(m) if matches!(m.feather, MaskFeather::LegacyCanvasFraction { .. }))
        }
        A::Mask => matches!(target, T::Mask(_)),
        A::ClassicMaskFontSize => matches!(target, T::Mask(m)
            if matches!(&m.shape, LayerMaskShape::Text { mask } if matches!(mask.font_size, FontSize::ClassicHeight90 { .. }))),
        A::PixelMaskFontSize => matches!(target, T::Mask(m)
            if matches!(&m.shape, LayerMaskShape::Text { mask } if matches!(mask.font_size, FontSize::CanvasPixels { .. }))),
        A::TextMask => {
            matches!(target, T::Mask(m) if matches!(m.shape, LayerMaskShape::Text { .. }))
        }
    }
}

pub fn scalar_applies(binding: &ScalarBinding, target: &ParameterTarget<'_>) -> bool {
    let requirement = match binding {
        ScalarBinding::Property { parameter } => scalar_metadata(*parameter).applies,
        ScalarBinding::ColorComponent {
            parameter,
            component,
        } => {
            let metadata = color_metadata(*parameter);
            metadata.component_unit(*component);
            metadata.applies
        }
    };
    applies(requirement, target)
}

pub fn discrete_applies(parameter: DiscreteParameter, target: &ParameterTarget<'_>) -> bool {
    applies(discrete_metadata(parameter).applies, target)
}
