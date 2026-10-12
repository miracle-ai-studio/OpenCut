//! Typed execution node preparation and explicit participation decisions.
use super::*;

pub(super) fn format(
    project: &ProjectDocumentV4,
    sequence: &SequenceV4,
) -> (CompiledFormatV4, BackgroundV4) {
    match &project.settings {
        SettingsProfileV4::ClassicGlobal { settings } => (
            CompiledFormatV4 {
                fps: settings.fps.clone(),
                canvas_size: settings.canvas_size,
                profile: FormatProfileV4::ClassicGlobal,
            },
            settings.background.clone(),
        ),
        SettingsProfileV4::LegacyPerScene { formats } => {
            let f = formats
                .iter()
                .find(|f| f.sequence_id == sequence.id)
                .expect("validated format coverage");
            (
                CompiledFormatV4 {
                    fps: f.fps.clone(),
                    canvas_size: f.canvas_size,
                    profile: FormatProfileV4::LegacyPerScene,
                },
                f.background.clone(),
            )
        }
    }
}
fn transition_range(
    value: &TransitionV4,
    sequence: &SequenceV4,
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
    sequence: &SequenceV4,
    at: RationalTime,
    entries: &mut Vec<SourceCoverageV4>,
) -> Result<Vec<CompiledTransitionV4>, CompositionError> {
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
                SourceNodeKindV4::Transition,
                ExclusionV4::DisabledElement,
            );
            continue;
        }
        if !contains(&range, at)? {
            coverage::exclude(
                entries,
                &t.id,
                SourceNodeKindV4::Transition,
                ExclusionV4::OutsideFrame,
            );
            continue;
        }
        let delta = at.checked_sub(time(&range.start)?).map_err(error)?;
        let duration = time(&range.duration)?;
        let progress = (delta.ticks() as f64 / duration.ticks() as f64)
            * (f64::from(duration.timebase()) / f64::from(delta.timebase()));
        output.push(CompiledTransitionV4 {
            id: t.id.clone(),
            outgoing_element_id: t.outgoing_element_id.clone(),
            incoming_element_id: t.incoming_element_id.clone(),
            profile: t.transition,
            range,
            incoming_weight: progress,
            outgoing_weight: 1.0,
        });
        coverage::emit(entries, &t.id, SourceNodeKindV4::Transition, &t.id);
    }
    Ok(output)
}
pub(super) fn media_asset<'a>(
    element: &TimelineElementV4,
    assets: &HashMap<&str, &'a AssetV4>,
) -> Option<&'a AssetV4> {
    let id = match &element.content {
        ElementContentV4::Video { asset_id, .. }
        | ElementContentV4::Audio { asset_id, .. }
        | ElementContentV4::Image { asset_id }
        | ElementContentV4::Sticker { asset_id, .. } => asset_id,
        _ => return None,
    };
    assets.get(id.as_str()).copied()
}

pub(super) fn audio_exclusion(
    element: &TimelineElementV4,
    track: &TrackV4,
    asset: Option<&AssetV4>,
    scope: CompositionScope,
) -> Option<ExclusionV4> {
    if scope == CompositionScope::Visual {
        return Some(ExclusionV4::VisualScope);
    }
    let (enabled, audio) = match &element.content {
        ElementContentV4::Video {
            source_audio_enabled,
            audio,
            ..
        } => (*source_audio_enabled, audio),
        ElementContentV4::Audio { audio, .. } => (true, audio),
        _ => return Some(ExclusionV4::SourceAudioDisabled),
    };
    if !enabled {
        Some(ExclusionV4::SourceAudioDisabled)
    } else if track.muted || audio.muted {
        Some(ExclusionV4::MutedAudio)
    } else if asset.is_some_and(|a| matches!(a.metadata.audio, AudioPresenceV4::Absent {})) {
        Some(ExclusionV4::KnownAbsentAudio)
    } else {
        None
    }
}
pub(super) fn visibility(
    element: &TimelineElementV4,
    transitions: &[CompiledTransitionV4],
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
pub(super) fn font_binding(value: &FontBindingV4, used: &mut HashSet<String>) {
    if let FontBindingV4::Locked { font_id } = value {
        used.insert(font_id.clone());
    }
}
pub(super) fn use_fonts(element: &TimelineElementV4, used: &mut HashSet<String>) {
    if let ElementContentV4::Text { text } = &element.content {
        font_binding(&text.font, used);
    }
    if let Some(v) = &element.visual {
        for mask in &v.masks {
            if mask.enabled
                && let MaskShapeV4::Text { mask } = &mask.shape
            {
                font_binding(&mask.font, used);
            }
        }
    }
}
pub(super) fn visual(
    element: &TimelineElementV4,
    track_opacity: f64,
    entries: &mut Vec<SourceCoverageV4>,
) -> Result<Option<SampledVisualV4>, CompositionError> {
    let Some(v) = &element.visual else {
        return Ok(None);
    };
    let mut effects = vec![];
    for e in &v.effects {
        if e.enabled {
            effects.push(e.clone());
            coverage::effect(entries, e, Some(&element.id), ExclusionV4::DisabledEffect);
        } else {
            coverage::effect(entries, e, None, ExclusionV4::DisabledEffect);
        }
    }
    let mut masks = vec![];
    for m in &v.masks {
        if m.enabled {
            masks.push(m.clone());
            coverage::mask(entries, m, Some(&element.id), ExclusionV4::DisabledMask);
        } else {
            coverage::mask(entries, m, None, ExclusionV4::DisabledMask);
        }
    }
    let legacy_shape = v.profile == VisualProfileV4::LegacyCompositionV3
        && matches!(element.content, ElementContentV4::Graphic { .. });
    Ok(Some(SampledVisualV4 {
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
            LayerOpacityPolicyV4::LegacyShapeElementOnly
        } else {
            LayerOpacityPolicyV4::MultiplyTrack
        },
        blend_mode: v.blend_mode,
        effects,
        masks,
    }))
}
pub(super) fn audio(
    element: &TimelineElementV4,
    track: &CompiledTrackV4,
    asset: &AssetV4,
    source: &SourceClockV4,
    scope: CompositionScope,
    output: &mut Vec<CompiledAudioV4>,
) -> Result<SourceAudioDecisionV4, CompositionError> {
    let (enabled, audio, retime) = match &element.content {
        ElementContentV4::Video {
            source_audio_enabled,
            audio,
            retime,
            ..
        } => (*source_audio_enabled, audio, retime),
        ElementContentV4::Audio { audio, retime, .. } => (true, audio, retime),
        _ => return Err(invalid("Audio execution requires an audio-bearing element")),
    };
    if scope == CompositionScope::Visual {
        return Ok(SourceAudioDecisionV4::ScopeExcluded);
    }
    if !enabled {
        return Ok(SourceAudioDecisionV4::Disabled);
    }
    if track.muted || audio.muted {
        return Ok(SourceAudioDecisionV4::Muted);
    }
    if matches!(asset.metadata.audio, AudioPresenceV4::Absent {}) {
        return Ok(SourceAudioDecisionV4::KnownAbsent);
    }
    let element_gain = animation::gain(audio.gain)?;
    let track_gain = track
        .gain
        .ok_or_else(|| invalid("Audio graph lacks sampled track gain"))?;
    output.push(CompiledAudioV4 {
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
            AudioPresenceV4::Unknown {} | AudioPresenceV4::Present { metadata: None }
        ) {
            SourceAudioDecisionV4::RequiresProbe
        } else {
            SourceAudioDecisionV4::Emitted
        },
    )
}
