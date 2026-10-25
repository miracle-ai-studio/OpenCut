//! Classic source-audio state changes over one complete V4 head. This plans
//! ordinary public edits; decoding, CAS reads and PCM capability are separate.
use crate::scene::{SceneCommitIntent, SceneRead, StoredProjectDocument};
use opencut_contracts::{EditorError, project_public::*, scene::*, scene_ops::*, source_audio::*};

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
    store: &impl SceneRead,
    input: &SourceAudioRequest,
) -> Result<SourceAudioPlan, EditorError> {
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
    if let SourceAudioCommand::Separate {
        audio_element_id,
        destination,
    } = &input.command
    {
        id(audio_element_id)?;
        match destination {
            SourceAudioDestination::Existing { track_id } => id(track_id)?,
            SourceAudioDestination::FirstAvailable { fallback_track_id } => id(fallback_track_id)?,
        }
    }
    let mut transaction = ApplyProject {
        branch_name: None,
        project_id: input.project_id.clone(),
        expected_revision: input.expected_revision.clone(),
        idempotency_key: input.idempotency_key.clone(),
        label: input.label.clone(),
        operations: vec![],
    };
    // This intent checks current Read/Edit/write-control before materializing
    // the head. The completed exact transaction is authorized again below.
    store.authorize_project(SceneCommitIntent::PublicApply(&transaction))?;
    let source = store.head_scene_source(&input.project_id)?;
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
    let StoredProjectDocument::Editable(project) = source.document else {
        return Err(EditorError::new(
            "UNSUPPORTED_PROJECT_VERSION",
            "Source audio requires a V4 head",
        ));
    };
    opencut_editor_core::scene::validate_project_scene(&project)?;
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
    let ElementContent::Video {
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
        AudioPresence::Unknown {} => {
            return Err(EditorError::new(
                "MEDIA_METADATA_REQUIRED",
                "Source audio presence must be probed before separation or restoration",
            ));
        }
        AudioPresence::Absent {} => {
            return Err(EditorError::new(
                "NO_AUDIO_TRACK",
                "Source media has no audio track",
            ));
        }
        AudioPresence::Present { .. } => {}
    }
    let next_enabled = matches!(input.command, SourceAudioCommand::Restore);
    if *source_audio_enabled == next_enabled {
        return Err(EditorError::new(
            "CONSTRAINT_VIOLATION",
            "Requested source audio state is already active",
        ));
    }
    let mut updated = original.clone();
    if let ElementContent::Video {
        source_audio_enabled,
        ..
    } = &mut updated.content
    {
        *source_audio_enabled = next_enabled;
    }
    let mut internal = vec![ProjectOperation::ElementPut {
        sequence_id: sequence.id.clone(),
        track_id: track.id.clone(),
        element: Box::new(updated.clone()),
    }];
    transaction
        .operations
        .push(ProjectEditOperation::ElementPut {
            sequence_id: sequence.id.clone(),
            track_id: track.id.clone(),
            element: Box::new(updated),
        });
    let mut detached = None;
    if let SourceAudioCommand::Separate {
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
        let element = TimelineElement {
            id: audio_element_id.clone(),
            name: original.name.clone(),
            timeline_range: original.timeline_range.clone(),
            source_span: original.source_span.clone(),
            enabled: true,
            content: ElementContent::Audio {
                asset_id: asset_id.clone(),
                audio: audio.clone(),
                retime: retime.clone(),
            },
            visual: None,
            channels,
        };
        let created = new_track.is_some();
        if let Some(new_track) = new_track {
            internal.push(ProjectOperation::TrackPut {
                sequence_id: sequence.id.clone(),
                track: new_track.clone(),
            });
            transaction.operations.push(ProjectEditOperation::TrackPut {
                sequence_id: sequence.id.clone(),
                track: new_track,
            });
        }
        internal.push(ProjectOperation::ElementPut {
            sequence_id: sequence.id.clone(),
            track_id: target.clone(),
            element: Box::new(element.clone()),
        });
        transaction
            .operations
            .push(ProjectEditOperation::ElementPut {
                sequence_id: sequence.id.clone(),
                track_id: target.clone(),
                element: Box::new(element),
            });
        detached = Some(DetachedAudioDecision {
            element_id: audio_element_id.clone(),
            track_id: target,
            track_created: created,
            copied_channels,
        });
    }
    let source_request =
        opencut_editor_core::scene::resolve_element_source_request_scene(original, Some(asset))?;
    let selected_asset_id = asset_id.clone();
    crate::scene::plan(
        &EditorSnapshot {
            project: *project,
            revision: source.revision,
        },
        &ApplySceneProject {
            project_id: transaction.project_id.clone(),
            expected_revision: transaction.expected_revision.clone(),
            idempotency_key: transaction.idempotency_key.clone(),
            label: transaction.label.clone(),
            operations: internal,
        },
    )?;
    store.authorize_project(SceneCommitIntent::PublicApply(&transaction))?;
    Ok(SourceAudioPlan {
        original_request_digest: crate::identity::request_identity(
            "project_v4_source_audio",
            &input.project_id,
            input,
        )?
        .current_digest()
        .into(),
        transaction,
        decision: SourceAudioDecision {
            source_element_id: input.element_id.clone(),
            asset_id: selected_asset_id,
            source_audio_enabled: next_enabled,
            source_request,
            detached,
        },
    })
}
