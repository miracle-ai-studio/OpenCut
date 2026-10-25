//! Authored clip gain/mute proposals over the complete fixed V4 head. Video
//! gain is an explicit product extension; no track line or DSP runs here.
use crate::scene::{SceneCommitIntent, SceneRead, StoredProjectDocument};
use opencut_contracts::{EditorError, audio_gain::*, project_public::*, scene::*, scene_ops::*};
#[cfg(test)]
mod tests;

fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
fn missing(message: &str) -> EditorError {
    EditorError::new("NOT_FOUND", message)
}
fn id(value: &str) -> Result<(), EditorError> {
    if value.trim().is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
        return Err(invalid("Audio gain needs bounded stable IDs"));
    }
    Ok(())
}
fn animated(element: &TimelineElement) -> bool {
    element.channels.iter().any(|channel| matches!(channel, AnimationChannel::Scalar {
        binding: ScalarBinding::Property { parameter: ScalarParameter::GainDecibels | ScalarParameter::GainLinearLegacy }, keys, ..
    } if !keys.is_empty()))
}

pub fn plan(
    store: &impl SceneRead,
    input: &AudioGainRequest,
) -> Result<AudioGainPlan, EditorError> {
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
        return Err(invalid("Audio gain needs a fixed revision and label"));
    }
    if let AudioGainCommand::SetDecibels { decibels_tenths } = input.command
        && !(-600..=200).contains(&decibels_tenths)
    {
        return Err(invalid("Clip gain must be -600..200 tenths dB"));
    }
    let mut transaction = ApplyProject {
        branch_name: None,
        project_id: input.project_id.clone(),
        expected_revision: input.expected_revision.clone(),
        idempotency_key: input.idempotency_key.clone(),
        label: input.label.clone(),
        operations: vec![],
    };
    store.authorize_project(SceneCommitIntent::PublicApply(&transaction))?;
    let source = store.head_scene_source(&input.project_id)?;
    if source.document.project_id() != input.project_id {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Audio gain head scope differs",
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
            "Audio gain requires a V4 head",
        ));
    };
    opencut_editor_core::scene::validate_project_scene(&project)?;
    let sequence = project
        .sequences
        .iter()
        .find(|s| s.id == input.sequence_id)
        .ok_or_else(|| missing("Audio gain scene does not exist"))?;
    let track = sequence
        .tracks
        .iter()
        .find(|t| t.id == input.track_id)
        .ok_or_else(|| missing("Audio gain track does not exist"))?;
    let original = track
        .elements
        .iter()
        .find(|e| e.id == input.element_id)
        .ok_or_else(|| missing("Audio gain element does not belong to this track"))?;
    let audio = match &original.content {
        ElementContent::Audio { audio, .. } => audio,
        ElementContent::Video {
            asset_id, audio, ..
        } => {
            let asset = project
                .assets
                .iter()
                .find(|a| a.id == *asset_id)
                .ok_or_else(|| missing("Source video asset does not exist"))?;
            match asset.metadata.audio {
                AudioPresence::Present { .. } => {}
                AudioPresence::Unknown {} => {
                    return Err(EditorError::new(
                        "MEDIA_METADATA_REQUIRED",
                        "Video audio presence must be known before authored gain editing",
                    ));
                }
                AudioPresence::Absent {} => {
                    return Err(EditorError::new(
                        "NO_AUDIO_TRACK",
                        "Source video has no audio track",
                    ));
                }
            }
            audio
        }
        _ => {
            return Err(invalid(
                "Audio gain requires an audio or known-audio video element",
            ));
        }
    };
    let has_animation = animated(original);
    let mut updated = original.clone();
    let next_audio = match &mut updated.content {
        ElementContent::Audio { audio, .. } | ElementContent::Video { audio, .. } => audio,
        _ => unreachable!(),
    };
    match input.command {
        AudioGainCommand::SetDecibels { decibels_tenths } => {
            if !matches!(audio.gain, Gain::Decibels { .. }) {
                return Err(EditorError::new(
                    "UNSUPPORTED_GAIN_PROFILE",
                    "Classic dB editing cannot reinterpret legacy linear gain",
                ));
            }
            if has_animation {
                return Err(EditorError::new(
                    "GAIN_ANIMATED",
                    "Static clip gain editing cannot replace a volume envelope",
                ));
            }
            next_audio.gain = Gain::Decibels {
                value: f64::from(decibels_tenths) / 10.0,
            };
        }
        AudioGainCommand::SetMuted { muted } => next_audio.muted = muted,
    }
    let decision = AudioGainDecision {
        element_id: original.id.clone(),
        old_gain: audio.gain,
        new_gain: next_audio.gain,
        old_element_muted: audio.muted,
        element_muted: next_audio.muted,
        track_muted: track.muted,
        animated: has_animation,
    };
    let changed = updated != *original;
    // Validate even no-change proposals through the shared lock/final-aggregate
    // rules. Returning None must not bypass a locked track or live authority.
    let operation = ProjectOperation::ElementPut {
        sequence_id: sequence.id.clone(),
        track_id: track.id.clone(),
        element: Box::new(updated.clone()),
    };
    transaction
        .operations
        .push(ProjectEditOperation::ElementPut {
            sequence_id: sequence.id.clone(),
            track_id: track.id.clone(),
            element: Box::new(updated),
        });
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
            operations: vec![operation],
        },
    )?;
    store.authorize_project(SceneCommitIntent::PublicApply(&transaction))?;
    Ok(AudioGainPlan {
        original_request_digest: crate::identity::request_identity(
            "project_v4_audio_gain",
            &input.project_id,
            input,
        )?
        .current_digest()
        .into(),
        decision,
        transaction: changed.then_some(transaction),
    })
}
