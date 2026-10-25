//! Bounded fixed-source animation observation through the existing pure sampler.
use crate::scene::{SceneRead, StoredProjectDocument};
use opencut_contracts::{EditorError, keyframe_sample::*, scene::AnimationChannel};
use opencut_editor_core::scene::{
    SampledChannel, sample_animation_channel_scene, validate_project_scene,
};
pub fn sample(
    store: &impl SceneRead,
    input: &KeyframeSampleRequest,
) -> Result<KeyframeSampleResult, EditorError> {
    let invalid = || {
        EditorError::new(
            "INVALID_ARGUMENT",
            "Sampling requires bounded fixed identities and 1..1024 local times",
        )
    };
    if [
        &input.project_id,
        &input.revision,
        &input.sequence_id,
        &input.channel_id,
    ]
    .iter()
    .any(|s| s.trim().is_empty() || s.len() > 128 || s.chars().any(char::is_control))
        || input.times.is_empty()
        || input.times.len() > 1024
    {
        return Err(invalid());
    }
    // The port authorizes reads before revealing the immutable document.
    let source = store.revision_scene_source(&input.project_id, &input.revision)?;
    if source.revision != input.revision || source.document.project_id() != input.project_id {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Fixed sampling scope changed",
        ));
    }
    let StoredProjectDocument::Editable(project) = source.document else {
        return Err(EditorError::new(
            "UNSUPPORTED_PROJECT_VERSION",
            "Sampling requires a V4 revision",
        ));
    };
    validate_project_scene(&project)?;
    let sequence = project
        .sequences
        .iter()
        .find(|s| s.id == input.sequence_id)
        .ok_or_else(|| EditorError::new("NOT_FOUND", "Sampling scene is missing"))?;
    let channel = sequence
        .tracks
        .iter()
        .flat_map(|t| {
            t.channels
                .iter()
                .chain(t.elements.iter().flat_map(|e| e.channels.iter()))
        })
        .find(|channel| match channel {
            AnimationChannel::Scalar { id, .. } | AnimationChannel::Discrete { id, .. } => {
                id == &input.channel_id
            }
        })
        .ok_or_else(|| EditorError::new("NOT_FOUND", "Sampling channel is missing"))?;
    let samples = input
        .times
        .iter()
        .map(|time| {
            let SampledChannel::Scalar { raw, physical, .. } =
                sample_animation_channel_scene(channel, time)?
            else {
                return Err(EditorError::new(
                    "INVALID_ARGUMENT",
                    "Only scalar channels have a numeric sampling observation",
                ));
            };
            Ok(KeyframeSample {
                time: time.clone(),
                value: raw,
                physical_value: physical,
            })
        })
        .collect::<Result<Vec<_>, EditorError>>()?;
    Ok(KeyframeSampleResult {
        revision: input.revision.clone(),
        channel_id: input.channel_id.clone(),
        samples,
    })
}
