use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceNodeKind {
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
pub enum Exclusion {
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
pub enum CoverageDisposition {
    Compiled { execution_id: String },
    Excluded { reason: Exclusion },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceCoverage {
    pub source_id: String,
    pub kind: SourceNodeKind,
    pub disposition: CoverageDisposition,
}

pub(super) fn emit(
    entries: &mut Vec<SourceCoverage>,
    id: &str,
    kind: SourceNodeKind,
    execution_id: &str,
) {
    entries.push(SourceCoverage {
        source_id: id.into(),
        kind,
        disposition: CoverageDisposition::Compiled {
            execution_id: execution_id.into(),
        },
    });
}
pub(super) fn exclude(
    entries: &mut Vec<SourceCoverage>,
    id: &str,
    kind: SourceNodeKind,
    reason: Exclusion,
) {
    entries.push(SourceCoverage {
        source_id: id.into(),
        kind,
        disposition: CoverageDisposition::Excluded { reason },
    });
}
pub(super) fn channel(
    entries: &mut Vec<SourceCoverage>,
    value: &AnimationChannel,
    execution: Option<&str>,
    reason: Exclusion,
) {
    let mut node = |id: &str, kind| match execution {
        Some(to) => emit(entries, id, kind, to),
        None => exclude(entries, id, kind, reason),
    };
    match value {
        AnimationChannel::Scalar { id, keys, .. } => {
            node(id, SourceNodeKind::Channel);
            for key in keys {
                node(&key.id, SourceNodeKind::Key);
                if let Some(id) = &key.segment_id {
                    node(id, SourceNodeKind::Segment);
                }
            }
        }
        AnimationChannel::Discrete { id, keys, .. } => {
            node(id, SourceNodeKind::Channel);
            for key in keys {
                node(&key.id, SourceNodeKind::Key);
            }
        }
    }
}
pub(super) fn effect(
    entries: &mut Vec<SourceCoverage>,
    effect: &SceneEffect,
    execution: Option<&str>,
    reason: Exclusion,
) {
    match execution {
        Some(to) => emit(entries, &effect.id, SourceNodeKind::Effect, to),
        None => exclude(entries, &effect.id, SourceNodeKind::Effect, reason),
    }
}
pub(super) fn mask(
    entries: &mut Vec<SourceCoverage>,
    mask: &Mask,
    execution: Option<&str>,
    reason: Exclusion,
) {
    match execution {
        Some(to) => emit(entries, &mask.id, SourceNodeKind::Mask, to),
        None => exclude(entries, &mask.id, SourceNodeKind::Mask, reason),
    }
    if let LayerMaskShape::Freeform { path } = &mask.shape {
        for point in &path.points {
            match execution {
                Some(to) => emit(entries, &point.id, SourceNodeKind::MaskPoint, to),
                None => exclude(entries, &point.id, SourceNodeKind::MaskPoint, reason),
            }
        }
    }
}
pub(super) fn element_excluded(
    entries: &mut Vec<SourceCoverage>,
    element: &TimelineElement,
    reason: Exclusion,
) {
    exclude(entries, &element.id, SourceNodeKind::Element, reason);
    if let Some(visual) = &element.visual {
        for node in &visual.effects {
            effect(entries, node, None, reason);
        }
        for node in &visual.masks {
            mask(entries, node, None, reason);
        }
    }
    if let ElementContent::EffectLayer { effect: node } = &element.content {
        effect(entries, node, None, reason);
    }
    for node in &element.channels {
        channel(entries, node, None, reason);
    }
}
pub(super) fn sequence_excluded(entries: &mut Vec<SourceCoverage>, sequence: &Scene) {
    let reason = Exclusion::OtherSequence;
    exclude(entries, &sequence.id, SourceNodeKind::Sequence, reason);
    for track in &sequence.tracks {
        exclude(entries, &track.id, SourceNodeKind::Track, reason);
        for element in &track.elements {
            element_excluded(entries, element, reason);
        }
        for value in &track.channels {
            channel(entries, value, None, reason);
        }
    }
    for value in &sequence.captions {
        exclude(entries, &value.id, SourceNodeKind::Caption, reason);
    }
    for value in &sequence.transitions {
        exclude(entries, &value.id, SourceNodeKind::Transition, reason);
    }
    for value in &sequence.bookmarks {
        exclude(entries, &value.id, SourceNodeKind::Bookmark, reason);
    }
}
