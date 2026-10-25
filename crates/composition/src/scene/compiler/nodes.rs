//! Typed execution node preparation and explicit participation decisions.
use super::*;

pub fn resolve_scene_format(
    project: &EditorDocument,
    sequence: &Scene,
) -> Result<(CompiledFormat, Background), CompositionError> {
    Ok(match &project.settings {
        SettingsProfile::ClassicGlobal { settings } => (
            CompiledFormat {
                fps: settings.fps.clone(),
                canvas_size: settings.canvas_size,
                profile: FormatProfile::ClassicGlobal,
            },
            settings.background.clone(),
        ),
        SettingsProfile::LegacyPerScene { formats } => {
            let f = formats
                .iter()
                .find(|f| f.sequence_id == sequence.id)
                .ok_or_else(|| invalid("Scene format is missing"))?;
            (
                CompiledFormat {
                    fps: f.fps.clone(),
                    canvas_size: f.canvas_size,
                    profile: FormatProfile::LegacyPerScene,
                },
                f.background.clone(),
            )
        }
    })
}
fn transition_range(
    value: &SceneTransition,
    sequence: &Scene,
) -> Result<TimeRange, CompositionError> {
    let incoming = sequence
        .tracks
        .iter()
        .flat_map(|t| &t.elements)
        .find(|e| e.id == value.incoming_element_id)
        .ok_or_else(|| invalid("Transition endpoint missing"))?;
    let half = time(&value.duration)?
        .checked_scale(
            RationalTime::new(1, 1).map_err(error)?,
            RationalTime::new(2, 1).map_err(error)?,
        )
        .map_err(error)?;
    Ok(TimeRange {
        start: time(&incoming.timeline_range.start)?
            .checked_sub(half)
            .map_err(error)?
            .into(),
        duration: value.duration.clone(),
    })
}
pub(super) fn active_transitions(
    sequence: &Scene,
    at: RationalTime,
    entries: &mut Vec<SourceCoverage>,
) -> Result<Vec<CompiledTransition>, CompositionError> {
    let mut output = vec![];
    for t in &sequence.transitions {
        let range = transition_range(t, sequence)?;
        let enabled = sequence
            .tracks
            .iter()
            .filter(|track| track.enabled)
            .flat_map(|track| &track.elements)
            .filter(|e| e.enabled)
            .filter(|e| e.id == t.outgoing_element_id || e.id == t.incoming_element_id)
            .count()
            == 2;
        if !enabled {
            coverage::exclude(
                entries,
                &t.id,
                SourceNodeKind::Transition,
                Exclusion::DisabledElement,
            );
            continue;
        }
        if !contains(&range, at)? {
            coverage::exclude(
                entries,
                &t.id,
                SourceNodeKind::Transition,
                Exclusion::OutsideFrame,
            );
            continue;
        }
        let delta = at.checked_sub(time(&range.start)?).map_err(error)?;
        let duration = time(&range.duration)?;
        let progress = (delta.ticks() as f64 / duration.ticks() as f64)
            * (f64::from(duration.timebase()) / f64::from(delta.timebase()));
        output.push(CompiledTransition {
            id: t.id.clone(),
            outgoing_element_id: t.outgoing_element_id.clone(),
            incoming_element_id: t.incoming_element_id.clone(),
            profile: t.transition,
            range,
            incoming_weight: progress,
            outgoing_weight: 1.0,
        });
        coverage::emit(entries, &t.id, SourceNodeKind::Transition, &t.id);
    }
    Ok(output)
}
pub(super) fn media_asset<'a>(
    element: &TimelineElement,
    assets: &HashMap<&str, &'a ProjectAsset>,
) -> Option<&'a ProjectAsset> {
    let id = match &element.content {
        ElementContent::Video { asset_id, .. }
        | ElementContent::Audio { asset_id, .. }
        | ElementContent::Image { asset_id }
        | ElementContent::Sticker { asset_id, .. } => asset_id,
        _ => return None,
    };
    assets.get(id.as_str()).copied()
}

pub(super) fn audio_exclusion(
    element: &TimelineElement,
    track: &SceneTrack,
    asset: Option<&ProjectAsset>,
    scope: CompositionScope,
) -> Option<Exclusion> {
    if scope == CompositionScope::Visual {
        return Some(Exclusion::VisualScope);
    }
    let (enabled, audio) = match &element.content {
        ElementContent::Video {
            source_audio_enabled,
            audio,
            ..
        } => (*source_audio_enabled, audio),
        ElementContent::Audio { audio, .. } => (true, audio),
        _ => return Some(Exclusion::SourceAudioDisabled),
    };
    if !enabled {
        Some(Exclusion::SourceAudioDisabled)
    } else if track.muted || audio.muted {
        Some(Exclusion::MutedAudio)
    } else if asset.is_some_and(|a| matches!(a.metadata.audio, AudioPresence::Absent {})) {
        Some(Exclusion::KnownAbsentAudio)
    } else {
        None
    }
}
pub(super) fn visibility(
    element: &TimelineElement,
    transitions: &[CompiledTransition],
) -> Result<(TimeRange, RationalTime, RationalTime), CompositionError> {
    let zero = RationalTime::new(0, 1).map_err(error)?;
    let mut head = zero;
    let mut tail = zero;
    for transition in transitions {
        let half = time(&transition.range.duration)?
            .checked_scale(
                RationalTime::new(1, 1).map_err(error)?,
                RationalTime::new(2, 1).map_err(error)?,
            )
            .map_err(error)?;
        if transition.incoming_element_id == element.id {
            head = half;
        }
        if transition.outgoing_element_id == element.id {
            tail = half;
        }
    }
    Ok((
        TimeRange {
            start: time(&element.timeline_range.start)?
                .checked_sub(head)
                .map_err(error)?
                .into(),
            duration: time(&element.timeline_range.duration)?
                .checked_add(head)
                .map_err(error)?
                .checked_add(tail)
                .map_err(error)?
                .into(),
        },
        head,
        tail,
    ))
}
pub(super) fn font_binding(value: &FontBinding, used: &mut HashSet<String>) {
    if let FontBinding::Locked { font_id } = value {
        used.insert(font_id.clone());
    }
}
pub(super) fn use_fonts(element: &TimelineElement, used: &mut HashSet<String>) {
    if let ElementContent::Text { text } = &element.content {
        font_binding(&text.font, used);
    }
    if let Some(v) = &element.visual {
        for mask in &v.masks {
            if mask.enabled
                && let LayerMaskShape::Text { mask } = &mask.shape
            {
                font_binding(&mask.font, used);
            }
        }
    }
}
pub(super) fn visual(
    element: &TimelineElement,
    track_opacity: f64,
    entries: &mut Vec<SourceCoverage>,
) -> Result<Option<SampledVisual>, CompositionError> {
    let Some(v) = &element.visual else {
        return Ok(None);
    };
    let mut effects = vec![];
    for e in &v.effects {
        if e.enabled {
            effects.push(e.clone());
            coverage::effect(entries, e, Some(&element.id), Exclusion::DisabledEffect);
        } else {
            coverage::effect(entries, e, None, Exclusion::DisabledEffect);
        }
    }
    let mut masks = vec![];
    for m in &v.masks {
        if m.enabled {
            masks.push(m.clone());
            coverage::mask(entries, m, Some(&element.id), Exclusion::DisabledMask);
        } else {
            coverage::mask(entries, m, None, Exclusion::DisabledMask);
        }
    }
    let legacy_shape = v.profile == VisualProfile::LegacyCompositionV3
        && matches!(element.content, ElementContent::Graphic { .. });
    Ok(Some(SampledVisual {
        profile: v.profile,
        transform: v.transform,
        element_opacity: v.opacity,
        track_opacity,
        effective_opacity: finite(if legacy_shape {
            v.opacity
        } else {
            v.opacity * track_opacity
        })?,
        opacity_policy: if legacy_shape {
            LayerOpacityPolicy::LegacyShapeElementOnly
        } else {
            LayerOpacityPolicy::MultiplyTrack
        },
        blend_mode: v.blend_mode,
        effects,
        masks,
    }))
}
pub(super) fn audio(
    element: &TimelineElement,
    track: &CompiledTrack,
    asset: &ProjectAsset,
    source: &SourceClock,
    scope: CompositionScope,
    output: &mut Vec<CompiledAudio>,
) -> Result<SourceAudioDecision, CompositionError> {
    let (enabled, audio, retime) = match &element.content {
        ElementContent::Video {
            source_audio_enabled,
            audio,
            retime,
            ..
        } => (*source_audio_enabled, audio, retime),
        ElementContent::Audio { audio, retime, .. } => (true, audio, retime),
        _ => return Err(invalid("Audio execution requires an audio-bearing element")),
    };
    if scope == CompositionScope::Visual {
        return Ok(SourceAudioDecision::ScopeExcluded);
    }
    if !enabled {
        return Ok(SourceAudioDecision::Disabled);
    }
    if track.muted || audio.muted {
        return Ok(SourceAudioDecision::Muted);
    }
    if matches!(asset.metadata.audio, AudioPresence::Absent {}) {
        return Ok(SourceAudioDecision::KnownAbsent);
    }
    let element_gain = animation::gain(audio.gain)?;
    let track_gain = track
        .gain
        .ok_or_else(|| invalid("Audio graph lacks sampled track gain"))?;
    output.push(CompiledAudio {
        element_id: element.id.clone(),
        track_id: track.id.clone(),
        asset_id: asset.id.clone(),
        stream: audio.stream,
        source: source.clone(),
        presence: asset.metadata.audio.clone(),
        maintain_pitch: retime.maintain_pitch,
        element_gain,
        track_gain,
        amplitude: finite(element_gain.amplitude * track_gain.amplitude)?,
    });
    Ok(
        if matches!(
            asset.metadata.audio,
            AudioPresence::Unknown {} | AudioPresence::Present { metadata: None }
        ) {
            SourceAudioDecision::RequiresProbe
        } else {
            SourceAudioDecision::Emitted
        },
    )
}
