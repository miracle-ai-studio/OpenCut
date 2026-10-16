//! Property inputs reuse the original public atomic transaction. Rate changes
//! preserve one source extent and use the existing exact/Classic inverse mapper;
//! channels keep their timeline-local clock; Classic clamp keeps the left side
//! and an explicit sampled boundary when the new duration shortens a segment.
use crate::project_v4::{
    StoredProjectDocumentV4, V4Intent,
    public::{ProjectV4PublicResources, plan_public},
};
use opencut_contracts::{
    EditorError, element_properties_v1::*, project_v4::*, project_v4_public::*,
};
use opencut_time::RationalTime as R;
fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
pub fn plan(
    store: &impl ProjectV4PublicResources,
    input: &ElementPropertiesRequestV1,
) -> Result<ApplyPublicV4Project, EditorError> {
    crate::require_idempotency_key(&input.idempotency_key)?;
    if input.label.trim().is_empty() || input.label.len() > 4096 {
        return Err(invalid("Property edit needs a bounded label"));
    }
    let mut transaction = ApplyPublicV4Project {
        project_id: input.project_id.clone(),
        expected_revision: input.expected_revision.clone(),
        idempotency_key: input.idempotency_key.clone(),
        label: input.label.clone(),
        operations: vec![],
    };
    store.authorize_v4(V4Intent::PublicApply(&transaction))?;
    let source = store.head_v4_source(&input.project_id)?;
    if source.revision != input.expected_revision {
        return Err(crate::revision_conflict(
            &input.expected_revision,
            &source.revision,
        ));
    }
    let StoredProjectDocumentV4::V4(project) = source.document else {
        return Err(EditorError::new(
            "UNSUPPORTED_PROJECT_VERSION",
            "Property editing requires V4",
        ));
    };
    if project.id != input.project_id {
        return Err(invalid("Property head scope differs"));
    }
    opencut_editor_core::project_v4::validate_project_v4(&project)?;
    let sequence = project
        .sequences
        .iter()
        .find(|s| s.id == input.sequence_id)
        .ok_or_else(|| invalid("Property scene does not exist"))?;
    let (track, original) = sequence
        .tracks
        .iter()
        .find_map(|t| {
            t.elements
                .iter()
                .find(|e| e.id == input.element_id)
                .map(|e| (t, e))
        })
        .ok_or_else(|| invalid("Property element does not exist"))?;
    if track.locked {
        return Err(EditorError::new(
            "TRACK_LOCKED",
            "Unlock the target track before editing",
        ));
    }
    let mut element = original.clone();
    if input.transform.is_some() || input.opacity.is_some() {
        let visual = element
            .visual
            .as_mut()
            .ok_or_else(|| invalid("This element has no visual transform or opacity"))?;
        if let Some(transform) = input.transform {
            visual.transform = transform;
        }
        if let Some(opacity) = input.opacity {
            visual.opacity = opacity;
        }
    }
    if input.audio_gain.is_some() || input.muted.is_some() {
        let audio = match &mut element.content {
            ElementContentV4::Video { audio, .. } | ElementContentV4::Audio { audio, .. } => audio,
            _ => return Err(invalid("This element has no audio gain or mute property")),
        };
        if let Some(gain) = input.audio_gain {
            audio.gain = gain;
        }
        if let Some(muted) = input.muted {
            audio.muted = muted;
        }
    }
    if let Some(next) = &input.retime {
        let previous = match &original.content {
            ElementContentV4::Video { retime, .. } | ElementContentV4::Audio { retime, .. } => {
                retime
            }
            _ => {
                return Err(invalid(
                    "Only video and audio have speed and pitch properties",
                ));
            }
        };
        if next.mapping != previous.mapping {
            return Err(invalid(
                "Speed edits preserve the element's existing source mapping profile",
            ));
        }
        if next.rate.numerator == 0 || next.rate.denominator == 0 {
            return Err(invalid("Speed must be a positive rational rate"));
        }
        if next.rate != previous.rate {
            if matches!(next.mapping, RetimeMappingV4::ClassicRoundedTicks { .. })
                && (u64::from(next.rate.numerator) * 100 < u64::from(next.rate.denominator)
                    || u64::from(next.rate.numerator) > u64::from(next.rate.denominator) * 5)
            {
                return Err(invalid("Classic speed authoring range is 0.01 through 5"));
            }

            let visible = match &original.source_span {
                ElementSourceSpanV4::ClassicTrimmed {
                    trim_start,
                    trim_end,
                    source_duration: Some(full),
                } => R::try_from(full)?
                    .checked_sub(R::try_from(trim_start)?)?
                    .checked_sub(R::try_from(trim_end)?)?,
                ElementSourceSpanV4::LegacySourceRange { range } => R::try_from(&range.duration)?,
                _ => opencut_editor_core::project_v4::map_retime_duration_v4(
                    R::try_from(&original.timeline_range.duration)?,
                    previous,
                    false,
                )?,
            };
            let duration =
                opencut_editor_core::project_v4::map_retime_duration_v4(visible, next, true)?;
            if duration.ticks() <= 0 {
                return Err(invalid("Speed change would create a zero-duration element"));
            }
            if !duration
                .cmp_exact(R::try_from(&original.timeline_range.duration)?)
                .is_eq()
            {
                element.timeline_range.duration = duration.into();
                element.channels = crate::timeline_v4::clamp_animation_duration_classic_v1(
                    &project,
                    &original.channels,
                    &element.timeline_range.duration,
                    &input.expected_revision,
                    &input.idempotency_key,
                    &original.id,
                )?;
            }
        }
        match &mut element.content {
            ElementContentV4::Video { retime, .. } | ElementContentV4::Audio { retime, .. } => {
                *retime = next.clone()
            }
            _ => unreachable!(),
        }
    }
    if element == *original {
        return Err(invalid("The property edit makes no change"));
    }
    transaction
        .operations
        .push(PublicProjectOperationV4::ElementPut {
            sequence_id: input.sequence_id.clone(),
            track_id: track.id.clone(),
            element: Box::new(element),
        });
    plan_public(store, &transaction)?;
    store.authorize_v4(V4Intent::PublicApply(&transaction))?;
    Ok(transaction)
}

#[cfg(test)]
mod tests;
