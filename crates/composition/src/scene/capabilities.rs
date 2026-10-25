//! Explicit executor contract. A pure graph is not evidence that an installed
//! renderer implements its required features. Hosts must gate before execution.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NativeFeature {
    FrameGraphV4,
    ExactSourceClock,
    CasVideo,
    CasImage,
    CasAudio,
    ResourceProbe,
    AudioFormatProbe,
    EofProfile,
    AudioRetimeMaintainPitch,
    AudioRetimeChangePitch,
    ClassicVisualProfile,
    LegacyVisualProfile,
    UnboundedLayerOpacity,
    NonNormalBlend,
    LinearSrgbColor,
    ClassicGraphics,
    LegacyGraphics,
    ClassicTitleLayout,
    PixelTitleLayout,
    CaptionLayoutLegacy,
    CaptionLayoutPinned,
    LockedFontFace,
    UnresolvedFontResolution,
    ClassicBlur,
    LegacyGaussianBlur,
    LegacyColorAdjust,
    EffectLayerBoundary,
    ClassicMaskGeometry,
    LegacyMaskGeometry,
    TextMask,
    FreeformMask,
    MaskFeatherPixels,
    MaskFeatherCanvasFraction,
    MaskStroke,
    MainTrackSourceBackground,
    LegacyCrossfade,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeRequirement {
    pub source_id: String,
    pub feature: NativeFeature,
}

fn require(output: &mut Vec<NativeRequirement>, id: &str, feature: NativeFeature) {
    if !output
        .iter()
        .any(|r| r.source_id == id && r.feature == feature)
    {
        output.push(NativeRequirement {
            source_id: id.into(),
            feature,
        });
    }
}
fn font(output: &mut Vec<NativeRequirement>, id: &str, value: &FontBinding) {
    require(
        output,
        id,
        match value {
            FontBinding::Locked { .. } => NativeFeature::LockedFontFace,
            FontBinding::UnresolvedFamily { .. } => NativeFeature::UnresolvedFontResolution,
        },
    );
}
fn color(output: &mut Vec<NativeRequirement>, id: &str, value: &Color) {
    if value.encoding == ColorEncoding::LinearSrgb {
        require(output, id, NativeFeature::LinearSrgbColor);
    }
}
fn effect(output: &mut Vec<NativeRequirement>, value: &SceneEffect) {
    require(
        output,
        &value.id,
        match value.effect {
            EffectKind::ClassicBlur { .. } => NativeFeature::ClassicBlur,
            EffectKind::LegacyGaussianBlur { .. } => NativeFeature::LegacyGaussianBlur,
            EffectKind::LegacyColorAdjust { .. } => NativeFeature::LegacyColorAdjust,
        },
    );
}
fn clock(output: &mut Vec<NativeRequirement>, id: &str, value: &SourceClock) {
    require(output, id, NativeFeature::ExactSourceClock);
    match value.sample {
        SourceReadState::InBounds => {}
        SourceReadState::NeedsResourceProbe => require(output, id, NativeFeature::ResourceProbe),
        SourceReadState::BeforeResource
        | SourceReadState::EndOfResource
        | SourceReadState::OutsideRequestedWindow => require(output, id, NativeFeature::EofProfile),
    }
}
pub fn required_capabilities_scene(graph: &SceneComposition) -> Vec<NativeRequirement> {
    let mut output = vec![];
    require(&mut output, &graph.project_id, NativeFeature::FrameGraphV4);
    for layer in &graph.layers {
        let id = &layer.element_id;
        if let Some(v) = &layer.visual {
            require(
                &mut output,
                id,
                match v.profile {
                    VisualProfile::ClassicCanvasV31 => NativeFeature::ClassicVisualProfile,
                    VisualProfile::LegacyCompositionV3 => NativeFeature::LegacyVisualProfile,
                },
            );
            if !(0.0..=1.0).contains(&v.effective_opacity) {
                require(&mut output, id, NativeFeature::UnboundedLayerOpacity);
            }
            if v.blend_mode != BlendMode::Normal {
                require(&mut output, id, NativeFeature::NonNormalBlend);
            }
            for e in &v.effects {
                effect(&mut output, e);
            }
            for m in &v.masks {
                let feature = match &m.shape {
                    LayerMaskShape::LegacyRectangle { .. }
                    | LayerMaskShape::LegacyEllipse { .. } => NativeFeature::LegacyMaskGeometry,
                    LayerMaskShape::Text { mask } => {
                        font(&mut output, &m.id, &mask.font);
                        NativeFeature::TextMask
                    }
                    LayerMaskShape::Freeform { .. } => NativeFeature::FreeformMask,
                    _ => NativeFeature::ClassicMaskGeometry,
                };
                require(&mut output, &m.id, feature);
                require(
                    &mut output,
                    &m.id,
                    match m.feather {
                        MaskFeather::ClassicPixels { .. } => NativeFeature::MaskFeatherPixels,
                        MaskFeather::LegacyCanvasFraction { .. } => {
                            NativeFeature::MaskFeatherCanvasFraction
                        }
                    },
                );
                if m.stroke.width_px != 0.0 {
                    require(&mut output, &m.id, NativeFeature::MaskStroke);
                }
                color(&mut output, &m.id, &m.stroke.color);
            }
        }
        match &layer.content {
            SampledContent::Video { source, .. } => {
                require(&mut output, id, NativeFeature::CasVideo);
                clock(&mut output, id, source);
            }
            SampledContent::Image { .. } | SampledContent::Sticker { .. } => {
                require(&mut output, id, NativeFeature::CasImage)
            }
            SampledContent::Text { text } => {
                font(&mut output, id, &text.font);
                require(
                    &mut output,
                    id,
                    match text.font_size {
                        FontSize::ClassicHeight90 { .. } => NativeFeature::ClassicTitleLayout,
                        FontSize::CanvasPixels { .. } => NativeFeature::PixelTitleLayout,
                    },
                );
                color(&mut output, id, &text.color);
                color(&mut output, id, &text.background.color);
            }
            SampledContent::Graphic { graphic } => {
                require(
                    &mut output,
                    id,
                    match graphic.geometry {
                        GraphicGeometry::LegacyRectangle { .. }
                        | GraphicGeometry::LegacyEllipse { .. } => NativeFeature::LegacyGraphics,
                        _ => NativeFeature::ClassicGraphics,
                    },
                );
                color(&mut output, id, &graphic.fill);
                color(&mut output, id, &graphic.stroke.color);
            }
            SampledContent::Caption { style, .. } => {
                font(&mut output, id, &style.font);
                require(
                    &mut output,
                    id,
                    match style.layout {
                        CaptionLayoutProfile::LegacyCaptionV2 => NativeFeature::CaptionLayoutLegacy,
                        CaptionLayoutProfile::PinnedCaptionV4 => NativeFeature::CaptionLayoutPinned,
                    },
                );
                color(&mut output, id, &style.color);
                if let Some(c) = &style.background_color {
                    color(&mut output, id, c);
                }
            }
            SampledContent::EffectLayer { effect: e } => {
                require(&mut output, id, NativeFeature::EffectLayerBoundary);
                effect(&mut output, e);
            }
        }
    }
    for audio in &graph.audio {
        require(&mut output, &audio.element_id, NativeFeature::CasAudio);
        clock(&mut output, &audio.element_id, &audio.source);
        if matches!(
            audio.presence,
            AudioPresence::Unknown {} | AudioPresence::Present { metadata: None }
        ) {
            require(
                &mut output,
                &audio.element_id,
                NativeFeature::AudioFormatProbe,
            );
        }
        if audio.source.rate.numerator != audio.source.rate.denominator {
            require(
                &mut output,
                &audio.element_id,
                if audio.maintain_pitch {
                    NativeFeature::AudioRetimeMaintainPitch
                } else {
                    NativeFeature::AudioRetimeChangePitch
                },
            );
        }
    }
    if let CompiledBackground::MainTrackBlur { .. } = &graph.background {
        require(
            &mut output,
            &graph.sequence_id,
            NativeFeature::MainTrackSourceBackground,
        );
    }
    if let CompiledBackground::Color { color: value } = &graph.background {
        color(&mut output, &graph.sequence_id, value);
    }
    for transition in &graph.transitions {
        require(&mut output, &transition.id, NativeFeature::LegacyCrossfade);
    }
    output
}

/// The host's advertised features must be implemented and independently tested;
/// passing all enum variants here is not a renderer implementation.
pub fn ensure_capabilities_scene(
    graph: &SceneComposition,
    supported: &[NativeFeature],
) -> Result<(), CompositionError> {
    let missing: Vec<_> = required_capabilities_scene(graph)
        .into_iter()
        .filter(|r| !supported.contains(&r.feature))
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(unsupported(format!(
            "Native V4 executor lacks required capabilities: {missing:?}"
        )))
    }
}
