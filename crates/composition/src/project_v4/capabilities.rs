//! Explicit executor contract. A pure graph is not evidence that an installed
//! renderer implements its required features. Hosts must gate before execution.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NativeFeatureV4 {
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
pub struct NativeRequirementV4 {
    pub source_id: String,
    pub feature: NativeFeatureV4,
}

fn require(output: &mut Vec<NativeRequirementV4>, id: &str, feature: NativeFeatureV4) {
    if !output
        .iter()
        .any(|r| r.source_id == id && r.feature == feature)
    {
        output.push(NativeRequirementV4 {
            source_id: id.into(),
            feature,
        });
    }
}
fn font(output: &mut Vec<NativeRequirementV4>, id: &str, value: &FontBindingV4) {
    require(
        output,
        id,
        match value {
            FontBindingV4::Locked { .. } => NativeFeatureV4::LockedFontFace,
            FontBindingV4::UnresolvedFamily { .. } => NativeFeatureV4::UnresolvedFontResolution,
        },
    );
}
fn color(output: &mut Vec<NativeRequirementV4>, id: &str, value: &ColorV4) {
    if value.encoding == ColorEncodingV4::LinearSrgb {
        require(output, id, NativeFeatureV4::LinearSrgbColor);
    }
}
fn effect(output: &mut Vec<NativeRequirementV4>, value: &EffectV4) {
    require(
        output,
        &value.id,
        match value.effect {
            EffectKindV4::ClassicBlur { .. } => NativeFeatureV4::ClassicBlur,
            EffectKindV4::LegacyGaussianBlur { .. } => NativeFeatureV4::LegacyGaussianBlur,
            EffectKindV4::LegacyColorAdjust { .. } => NativeFeatureV4::LegacyColorAdjust,
        },
    );
}
fn clock(output: &mut Vec<NativeRequirementV4>, id: &str, value: &SourceClockV4) {
    require(output, id, NativeFeatureV4::ExactSourceClock);
    match value.sample {
        SourceReadStateV4::InBounds => {}
        SourceReadStateV4::NeedsResourceProbe => {
            require(output, id, NativeFeatureV4::ResourceProbe)
        }
        SourceReadStateV4::BeforeResource
        | SourceReadStateV4::EndOfResource
        | SourceReadStateV4::OutsideRequestedWindow => {
            require(output, id, NativeFeatureV4::EofProfile)
        }
    }
}
pub fn required_capabilities_v4(graph: &CompositionV4) -> Vec<NativeRequirementV4> {
    let mut output = vec![];
    require(
        &mut output,
        &graph.project_id,
        NativeFeatureV4::FrameGraphV4,
    );
    for layer in &graph.layers {
        let id = &layer.element_id;
        if let Some(v) = &layer.visual {
            require(
                &mut output,
                id,
                match v.profile {
                    VisualProfileV4::ClassicCanvasV31 => NativeFeatureV4::ClassicVisualProfile,
                    VisualProfileV4::LegacyCompositionV3 => NativeFeatureV4::LegacyVisualProfile,
                },
            );
            if !(0.0..=1.0).contains(&v.effective_opacity) {
                require(&mut output, id, NativeFeatureV4::UnboundedLayerOpacity);
            }
            if v.blend_mode != BlendModeV4::Normal {
                require(&mut output, id, NativeFeatureV4::NonNormalBlend);
            }
            for e in &v.effects {
                effect(&mut output, e);
            }
            for m in &v.masks {
                let feature = match &m.shape {
                    MaskShapeV4::LegacyRectangle { .. } | MaskShapeV4::LegacyEllipse { .. } => {
                        NativeFeatureV4::LegacyMaskGeometry
                    }
                    MaskShapeV4::Text { mask } => {
                        font(&mut output, &m.id, &mask.font);
                        NativeFeatureV4::TextMask
                    }
                    MaskShapeV4::Freeform { .. } => NativeFeatureV4::FreeformMask,
                    _ => NativeFeatureV4::ClassicMaskGeometry,
                };
                require(&mut output, &m.id, feature);
                require(
                    &mut output,
                    &m.id,
                    match m.feather {
                        MaskFeatherV4::ClassicPixels { .. } => NativeFeatureV4::MaskFeatherPixels,
                        MaskFeatherV4::LegacyCanvasFraction { .. } => {
                            NativeFeatureV4::MaskFeatherCanvasFraction
                        }
                    },
                );
                if m.stroke.width_px != 0.0 {
                    require(&mut output, &m.id, NativeFeatureV4::MaskStroke);
                }
                color(&mut output, &m.id, &m.stroke.color);
            }
        }
        match &layer.content {
            SampledContentV4::Video { source, .. } => {
                require(&mut output, id, NativeFeatureV4::CasVideo);
                clock(&mut output, id, source);
            }
            SampledContentV4::Image { .. } | SampledContentV4::Sticker { .. } => {
                require(&mut output, id, NativeFeatureV4::CasImage)
            }
            SampledContentV4::Text { text } => {
                font(&mut output, id, &text.font);
                require(
                    &mut output,
                    id,
                    match text.font_size {
                        FontSizeV4::ClassicHeight90 { .. } => NativeFeatureV4::ClassicTitleLayout,
                        FontSizeV4::CanvasPixels { .. } => NativeFeatureV4::PixelTitleLayout,
                    },
                );
                color(&mut output, id, &text.color);
                color(&mut output, id, &text.background.color);
            }
            SampledContentV4::Graphic { graphic } => {
                require(
                    &mut output,
                    id,
                    match graphic.geometry {
                        GraphicGeometryV4::LegacyRectangle { .. }
                        | GraphicGeometryV4::LegacyEllipse { .. } => {
                            NativeFeatureV4::LegacyGraphics
                        }
                        _ => NativeFeatureV4::ClassicGraphics,
                    },
                );
                color(&mut output, id, &graphic.fill);
                color(&mut output, id, &graphic.stroke.color);
            }
            SampledContentV4::Caption { style, .. } => {
                font(&mut output, id, &style.font);
                require(
                    &mut output,
                    id,
                    match style.layout {
                        CaptionLayoutProfileV4::LegacyCaptionV2 => {
                            NativeFeatureV4::CaptionLayoutLegacy
                        }
                        CaptionLayoutProfileV4::PinnedCaptionV4 => {
                            NativeFeatureV4::CaptionLayoutPinned
                        }
                    },
                );
                color(&mut output, id, &style.color);
                if let Some(c) = &style.background_color {
                    color(&mut output, id, c);
                }
            }
            SampledContentV4::EffectLayer { effect: e } => {
                require(&mut output, id, NativeFeatureV4::EffectLayerBoundary);
                effect(&mut output, e);
            }
        }
    }
    for audio in &graph.audio {
        require(&mut output, &audio.element_id, NativeFeatureV4::CasAudio);
        clock(&mut output, &audio.element_id, &audio.source);
        if matches!(
            audio.presence,
            AudioPresenceV4::Unknown {} | AudioPresenceV4::Present { metadata: None }
        ) {
            require(
                &mut output,
                &audio.element_id,
                NativeFeatureV4::AudioFormatProbe,
            );
        }
        if audio.source.rate.numerator != audio.source.rate.denominator {
            require(
                &mut output,
                &audio.element_id,
                if audio.maintain_pitch {
                    NativeFeatureV4::AudioRetimeMaintainPitch
                } else {
                    NativeFeatureV4::AudioRetimeChangePitch
                },
            );
        }
    }
    if let CompiledBackgroundV4::MainTrackBlur { .. } = &graph.background {
        require(
            &mut output,
            &graph.sequence_id,
            NativeFeatureV4::MainTrackSourceBackground,
        );
    }
    if let CompiledBackgroundV4::Color { color: value } = &graph.background {
        color(&mut output, &graph.sequence_id, value);
    }
    for transition in &graph.transitions {
        require(
            &mut output,
            &transition.id,
            NativeFeatureV4::LegacyCrossfade,
        );
    }
    output
}

/// The host's advertised features must be implemented and independently tested;
/// passing all enum variants here is not a renderer implementation.
pub fn ensure_capabilities_v4(
    graph: &CompositionV4,
    supported: &[NativeFeatureV4],
) -> Result<(), CompositionError> {
    let missing: Vec<_> = required_capabilities_v4(graph)
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
