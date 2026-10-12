use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceNodeKindV4 {
    Project,
    Settings,
    Asset,
    Font,
    Sequence,
    Track,
    Element,
    Caption,
    Effect,
    Mask,
    MaskPoint,
    Channel,
    Key,
    Segment,
    Transition,
    Bookmark,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExclusionV4 {
    OtherSequence,
    DisabledTrack,
    DisabledElement,
    DisabledEffect,
    DisabledMask,
    OutsideFrame,
    VisualScope,
    EditorialOnly,
    UnusedResource,
    MutedAudio,
    KnownAbsentAudio,
    SourceAudioDisabled,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoverageDispositionV4 {
    Compiled { execution_id: String },
    Excluded { reason: ExclusionV4 },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceCoverageV4 {
    pub source_id: String,
    pub kind: SourceNodeKindV4,
    pub disposition: CoverageDispositionV4,
}

pub(super) fn emit(
    entries: &mut Vec<SourceCoverageV4>,
    id: &str,
    kind: SourceNodeKindV4,
    execution_id: &str,
) {
    entries.push(SourceCoverageV4 {
        source_id: id.into(),
        kind,
        disposition: CoverageDispositionV4::Compiled {
            execution_id: execution_id.into(),
        },
    });
}
pub(super) fn exclude(
    entries: &mut Vec<SourceCoverageV4>,
    id: &str,
    kind: SourceNodeKindV4,
    reason: ExclusionV4,
) {
    entries.push(SourceCoverageV4 {
        source_id: id.into(),
        kind,
        disposition: CoverageDispositionV4::Excluded { reason },
    });
}
pub(super) fn channel(
    entries: &mut Vec<SourceCoverageV4>,
    value: &AnimationChannelV4,
    execution: Option<&str>,
    reason: ExclusionV4,
) {
    let mut node = |id: &str, kind| match execution {
        Some(to) => emit(entries, id, kind, to),
        None => exclude(entries, id, kind, reason),
    };
    match value {
        AnimationChannelV4::Scalar { id, keys, .. } => {
            node(id, SourceNodeKindV4::Channel);
            for key in keys {
                node(&key.id, SourceNodeKindV4::Key);
                if let Some(id) = &key.segment_id {
                    node(id, SourceNodeKindV4::Segment);
                }
            }
        }
        AnimationChannelV4::Discrete { id, keys, .. } => {
            node(id, SourceNodeKindV4::Channel);
            for key in keys {
                node(&key.id, SourceNodeKindV4::Key);
            }
        }
    }
}
pub(super) fn effect(
    entries: &mut Vec<SourceCoverageV4>,
    effect: &EffectV4,
    execution: Option<&str>,
    reason: ExclusionV4,
) {
    match execution {
        Some(to) => emit(entries, &effect.id, SourceNodeKindV4::Effect, to),
        None => exclude(entries, &effect.id, SourceNodeKindV4::Effect, reason),
    }
}
pub(super) fn mask(
    entries: &mut Vec<SourceCoverageV4>,
    mask: &MaskV4,
    execution: Option<&str>,
    reason: ExclusionV4,
) {
    match execution {
        Some(to) => emit(entries, &mask.id, SourceNodeKindV4::Mask, to),
        None => exclude(entries, &mask.id, SourceNodeKindV4::Mask, reason),
    }
    if let MaskShapeV4::Freeform { path } = &mask.shape {
        for point in &path.points {
            match execution {
                Some(to) => emit(entries, &point.id, SourceNodeKindV4::MaskPoint, to),
                None => exclude(entries, &point.id, SourceNodeKindV4::MaskPoint, reason),
            }
        }
    }
}
pub(super) fn element_excluded(
    entries: &mut Vec<SourceCoverageV4>,
    element: &TimelineElementV4,
    reason: ExclusionV4,
) {
    exclude(entries, &element.id, SourceNodeKindV4::Element, reason);
    if let Some(visual) = &element.visual {
        for node in &visual.effects {
            effect(entries, node, None, reason);
        }
        for node in &visual.masks {
            mask(entries, node, None, reason);
        }
    }
    if let ElementContentV4::EffectLayer { effect: node } = &element.content {
        effect(entries, node, None, reason);
    }
    for node in &element.channels {
        channel(entries, node, None, reason);
    }
}
pub(super) fn sequence_excluded(entries: &mut Vec<SourceCoverageV4>, sequence: &SequenceV4) {
    let reason = ExclusionV4::OtherSequence;
    exclude(entries, &sequence.id, SourceNodeKindV4::Sequence, reason);
    for track in &sequence.tracks {
        exclude(entries, &track.id, SourceNodeKindV4::Track, reason);
        for element in &track.elements {
            element_excluded(entries, element, reason);
        }
        for value in &track.channels {
            channel(entries, value, None, reason);
        }
    }
    for value in &sequence.captions {
        exclude(entries, &value.id, SourceNodeKindV4::Caption, reason);
    }
    for value in &sequence.transitions {
        exclude(entries, &value.id, SourceNodeKindV4::Transition, reason);
    }
    for value in &sequence.bookmarks {
        exclude(entries, &value.id, SourceNodeKindV4::Bookmark, reason);
    }
}
