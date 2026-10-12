//! Shared parameter/target matrix. Ownership resolution remains in channels.
use super::*;

#[derive(Clone, Copy, Debug)]
pub enum ParameterTargetV4<'a> {
    Element(&'a TimelineElementV4),
    Effect(&'a EffectV4),
    Mask(&'a MaskV4),
    Track(&'a TrackV4),
}

fn element_gain(element: &TimelineElementV4) -> Option<GainV4> {
    match &element.content {
        ElementContentV4::Video { audio, .. } | ElementContentV4::Audio { audio, .. } => {
            Some(audio.gain)
        }
        _ => None,
    }
}
fn box_shape(shape: &MaskShapeV4) -> bool {
    match shape {
        MaskShapeV4::CinematicBars { .. }
        | MaskShapeV4::Rectangle { .. }
        | MaskShapeV4::Ellipse { .. }
        | MaskShapeV4::Heart { .. }
        | MaskShapeV4::Diamond { .. }
        | MaskShapeV4::Star { .. } => true,
        MaskShapeV4::Split { .. }
        | MaskShapeV4::Text { .. }
        | MaskShapeV4::Freeform { .. }
        | MaskShapeV4::LegacyRectangle { .. }
        | MaskShapeV4::LegacyEllipse { .. } => false,
    }
}
fn applies(requirement: ParameterApplicabilityV4, target: &ParameterTargetV4<'_>) -> bool {
    use ParameterApplicabilityV4 as A;
    use ParameterTargetV4 as T;
    match requirement {
        A::VisualElement => matches!(target, T::Element(e) if e.visual.is_some()),
        A::VisualElementOrTrack => {
            matches!(target, T::Element(e) if e.visual.is_some())
                || matches!(target, T::Track(t) if t.kind != TrackKindV4::Audio)
        }
        A::LinearGain | A::DecibelGain => {
            let gain = match target {
                T::Element(e) => element_gain(e),
                T::Track(t) if matches!(t.kind, TrackKindV4::Video | TrackKindV4::Audio) => {
                    Some(t.gain)
                }
                T::Track(_) | T::Effect(_) | T::Mask(_) => None,
            };
            matches!(
                (requirement, gain),
                (A::LinearGain, Some(GainV4::LinearLegacy { .. }))
                    | (A::DecibelGain, Some(GainV4::Decibels { .. }))
            )
        }
        A::ClassicTextFontSize => matches!(target, T::Element(e)
            if matches!(&e.content, ElementContentV4::Text { text } if matches!(text.font_size, FontSizeV4::ClassicHeight90 { .. }))),
        A::PixelTextFontSize => matches!(target, T::Element(e)
            if matches!(&e.content, ElementContentV4::Text { text } if matches!(text.font_size, FontSizeV4::CanvasPixels { .. }))),
        A::Text => {
            matches!(target, T::Element(e) if matches!(e.content, ElementContentV4::Text { .. }))
        }
        A::Graphic => {
            matches!(target, T::Element(e) if matches!(e.content, ElementContentV4::Graphic { .. }))
        }
        A::GraphicRectangleOrPolygon => matches!(target, T::Element(e)
            if matches!(&e.content, ElementContentV4::Graphic { graphic } if matches!(graphic.geometry, GraphicGeometryV4::Rectangle { .. } | GraphicGeometryV4::Polygon { .. }))),
        A::GraphicPolygon => matches!(target, T::Element(e)
            if matches!(&e.content, ElementContentV4::Graphic { graphic } if matches!(graphic.geometry, GraphicGeometryV4::Polygon { .. }))),
        A::GraphicStar => matches!(target, T::Element(e)
            if matches!(&e.content, ElementContentV4::Graphic { graphic } if matches!(graphic.geometry, GraphicGeometryV4::Star { .. }))),
        A::ClassicBlurEffect => {
            matches!(target, T::Effect(e) if matches!(e.effect, EffectKindV4::ClassicBlur { .. }))
        }
        A::LegacyBlurEffect => {
            matches!(target, T::Effect(e) if matches!(e.effect, EffectKindV4::LegacyGaussianBlur { .. }))
        }
        A::LegacyColorEffect => {
            matches!(target, T::Effect(e) if matches!(e.effect, EffectKindV4::LegacyColorAdjust { .. }))
        }
        A::ClassicMaskGeometry => matches!(target, T::Mask(m)
            if !matches!(m.shape, MaskShapeV4::LegacyRectangle { .. } | MaskShapeV4::LegacyEllipse { .. })),
        A::BoxMask => matches!(target, T::Mask(m) if box_shape(&m.shape)),
        A::ScalableMask => matches!(target, T::Mask(m)
            if box_shape(&m.shape) || matches!(m.shape, MaskShapeV4::Text { .. } | MaskShapeV4::Freeform { .. })),
        A::ClassicMaskFeather => {
            matches!(target, T::Mask(m) if matches!(m.feather, MaskFeatherV4::ClassicPixels { .. }))
        }
        A::LegacyMaskFeather => {
            matches!(target, T::Mask(m) if matches!(m.feather, MaskFeatherV4::LegacyCanvasFraction { .. }))
        }
        A::Mask => matches!(target, T::Mask(_)),
        A::ClassicMaskFontSize => matches!(target, T::Mask(m)
            if matches!(&m.shape, MaskShapeV4::Text { mask } if matches!(mask.font_size, FontSizeV4::ClassicHeight90 { .. }))),
        A::PixelMaskFontSize => matches!(target, T::Mask(m)
            if matches!(&m.shape, MaskShapeV4::Text { mask } if matches!(mask.font_size, FontSizeV4::CanvasPixels { .. }))),
        A::TextMask => matches!(target, T::Mask(m) if matches!(m.shape, MaskShapeV4::Text { .. })),
    }
}

pub fn scalar_applies(binding: &ScalarBindingV4, target: &ParameterTargetV4<'_>) -> bool {
    let requirement = match binding {
        ScalarBindingV4::Property { parameter } => scalar_metadata(*parameter).applies,
        ScalarBindingV4::ColorComponent {
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

pub fn discrete_applies(parameter: DiscreteParameterV4, target: &ParameterTargetV4<'_>) -> bool {
    applies(discrete_metadata(parameter).applies, target)
}
