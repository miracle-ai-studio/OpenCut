//! Classic source-audio state changes over one complete V4 head. This plans
//! ordinary public edits; decoding, CAS reads and PCM capability are separate.
use crate::project_v4::{ProjectV4Read, StoredProjectDocumentV4, V4Intent};
use opencut_contracts::{
    EditorError, project_v4::*, project_v4_ops::*, project_v4_public::*, source_audio_v4::*,
};

mod channels;
mod placement;
#[cfg(test)]
mod tests;

fn invalid(message: impl Into<String>) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
fn missing(message: impl Into<String>) -> EditorError {
    EditorError::new("NOT_FOUND", message)
}
fn id(value: &str) -> Result<(), EditorError> {
    if value.trim().is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
        return Err(invalid("Source audio needs bounded stable IDs"));
    }
    Ok(())
}

pub fn plan(
    store: &impl ProjectV4Read,
    input: &SourceAudioRequestV4,
) -> Result<SourceAudioPlanV4, EditorError> {
    crate::require_idempotency_key(&input.idempotency_key)?;
    for value in [
        &input.project_id,
        &input.sequence_id,
        &input.track_id,
        &input.element_id,
    ] {
        id(value)?;
    }
    if input.expected_revision.trim().is_empty()
        || input.label.trim().is_empty()
        || input.label.len() > 4096
    {
        return Err(invalid("Source audio needs a fixed revision and label"));
    }
    if let SourceAudioCommandV4::Separate {
        audio_element_id,
        destination,
    } = &input.command
    {
        id(audio_element_id)?;
        match destination {
            SourceAudioDestinationV4::Existing { track_id } => id(track_id)?,
            SourceAudioDestinationV4::FirstAvailable { fallback_track_id } => {
                id(fallback_track_id)?
            }
        }
    }
    let mut transaction = ApplyPublicV4Project {
        project_id: input.project_id.clone(),
        expected_revision: input.expected_revision.clone(),
        idempotency_key: input.idempotency_key.clone(),
        label: input.label.clone(),
        operations: vec![],
    };
    // This intent checks current Read/Edit/write-control before materializing
    // the head. The completed exact transaction is authorized again below.
    store.authorize_v4(V4Intent::PublicApply(&transaction))?;
    let source = store.head_v4_source(&input.project_id)?;
    if source.document.project_id() != input.project_id {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Source audio head scope differs",
        ));
    }
    if source.revision != input.expected_revision {
        return Err(crate::revision_conflict(
            &input.expected_revision,
            &source.revision,
        ));
    }
    let StoredProjectDocumentV4::V4(project) = source.document else {
        return Err(EditorError::new(
            "UNSUPPORTED_PROJECT_VERSION",
            "Source audio requires a V4 head",
        ));
    };
    opencut_editor_core::project_v4::validate_project_v4(&project)?;
    let sequence = project
        .sequences
        .iter()
        .find(|s| s.id == input.sequence_id)
        .ok_or_else(|| missing("Source audio scene does not exist"))?;
    let track = sequence
        .tracks
        .iter()
        .find(|t| t.id == input.track_id)
        .ok_or_else(|| missing("Source audio track does not exist"))?;
    let original = track
        .elements
        .iter()
        .find(|e| e.id == input.element_id)
        .ok_or_else(|| missing("Source audio element does not belong to this track"))?;
    let ElementContentV4::Video {
        asset_id,
        source_audio_enabled,
        audio,
        retime,
        ..
    } = &original.content
    else {
        return Err(invalid("Source audio requires a video element"));
    };
    let asset = project
        .assets
        .iter()
        .find(|a| a.id == *asset_id)
        .ok_or_else(|| missing("Source video asset does not exist"))?;
    match asset.metadata.audio {
        AudioPresenceV4::Unknown {} => {
            return Err(EditorError::new(
                "MEDIA_METADATA_REQUIRED",
                "Source audio presence must be probed before separation or restoration",
            ));
        }
        AudioPresenceV4::Absent {} => {
            return Err(EditorError::new(
                "NO_AUDIO_TRACK",
                "Source media has no audio track",
            ));
        }
        AudioPresenceV4::Present { .. } => {}
    }
    let next_enabled = matches!(input.command, SourceAudioCommandV4::Restore);
    if *source_audio_enabled == next_enabled {
        return Err(EditorError::new(
            "CONSTRAINT_VIOLATION",
            "Requested source audio state is already active",
        ));
    }
    let mut updated = original.clone();
    if let ElementContentV4::Video {
        source_audio_enabled,
        ..
    } = &mut updated.content
    {
        *source_audio_enabled = next_enabled;
    }
    let mut internal = vec![ProjectOperationV4::ElementPut {
        sequence_id: sequence.id.clone(),
        track_id: track.id.clone(),
        element: Box::new(updated.clone()),
    }];
    transaction
        .operations
        .push(PublicProjectOperationV4::ElementPut {
            sequence_id: sequence.id.clone(),
            track_id: track.id.clone(),
            element: Box::new(updated),
        });
    let mut detached = None;
    if let SourceAudioCommandV4::Separate {
        audio_element_id,
        destination,
    } = &input.command
    {
        let (target, new_track) = placement::target(sequence, original, destination)?;
        let (channels, copied_channels) = channels::copy(
            &project,
            original,
            audio_element_id,
            new_track.as_ref().map(|t| t.id.as_str()),
            &source.revision,
        )?;
        let element = TimelineElementV4 {
            id: audio_element_id.clone(),
            name: original.name.clone(),
            timeline_range: original.timeline_range.clone(),
            source_span: original.source_span.clone(),
            enabled: true,
            content: ElementContentV4::Audio {
                asset_id: asset_id.clone(),
                audio: audio.clone(),
                retime: retime.clone(),
            },
            visual: None,
            channels,
        };
        let created = new_track.is_some();
        if let Some(new_track) = new_track {
            internal.push(ProjectOperationV4::TrackPut {
                sequence_id: sequence.id.clone(),
                track: new_track.clone(),
            });
            transaction
                .operations
                .push(PublicProjectOperationV4::TrackPut {
                    sequence_id: sequence.id.clone(),
                    track: new_track,
                });
        }
        internal.push(ProjectOperationV4::ElementPut {
            sequence_id: sequence.id.clone(),
            track_id: target.clone(),
            element: Box::new(element.clone()),
        });
        transaction
            .operations
            .push(PublicProjectOperationV4::ElementPut {
                sequence_id: sequence.id.clone(),
                track_id: target.clone(),
                element: Box::new(element),
            });
        detached = Some(DetachedAudioDecisionV4 {
            element_id: audio_element_id.clone(),
            track_id: target,
            track_created: created,
            copied_channels,
        });
    }
    let source_request =
        opencut_editor_core::project_v4::resolve_element_source_request_v4(original, Some(asset))?;
    let selected_asset_id = asset_id.clone();
    crate::project_v4::plan(
        &ProjectSnapshotV4 {
            project: *project,
            revision: source.revision,
        },
        &ApplyV4Project {
            project_id: transaction.project_id.clone(),
            expected_revision: transaction.expected_revision.clone(),
            idempotency_key: transaction.idempotency_key.clone(),
            label: transaction.label.clone(),
            operations: internal,
        },
    )?;
    store.authorize_v4(V4Intent::PublicApply(&transaction))?;
    Ok(SourceAudioPlanV4 {
        original_request_digest: crate::identity::request_identity(
            "project_v4_source_audio",
            &input.project_id,
            input,
        )?
        .current_digest()
        .into(),
        transaction,
        decision: SourceAudioDecisionV4 {
            source_element_id: input.element_id.clone(),
            asset_id: selected_asset_id,
            source_audio_enabled: next_enabled,
            source_request,
            detached,
        },
    })
}
