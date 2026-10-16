//! Bounded fixed-source animation observation through the existing pure sampler.
use crate::project_v4::{ProjectV4Read, StoredProjectDocumentV4};
use opencut_contracts::{EditorError, keyframe_sample_v1::*, project_v4::AnimationChannelV4};
use opencut_editor_core::project_v4::{
    SampledChannelV4, sample_animation_channel_v4, validate_project_v4,
};
pub fn sample(
    store: &impl ProjectV4Read,
    input: &KeyframeSampleRequestV1,
) -> Result<KeyframeSampleResultV1, EditorError> {
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
    let source = store.revision_v4_source(&input.project_id, &input.revision)?;
    if source.revision != input.revision || source.document.project_id() != input.project_id {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Fixed sampling scope changed",
        ));
    }
    let StoredProjectDocumentV4::V4(project) = source.document else {
        return Err(EditorError::new(
            "UNSUPPORTED_PROJECT_VERSION",
            "Sampling requires a V4 revision",
        ));
    };
    validate_project_v4(&project)?;
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
            AnimationChannelV4::Scalar { id, .. } | AnimationChannelV4::Discrete { id, .. } => {
                id == &input.channel_id
            }
        })
        .ok_or_else(|| EditorError::new("NOT_FOUND", "Sampling channel is missing"))?;
    let samples = input
        .times
        .iter()
        .map(|time| {
            let SampledChannelV4::Scalar { raw, physical, .. } =
                sample_animation_channel_v4(channel, time)?
            else {
                return Err(EditorError::new(
                    "INVALID_ARGUMENT",
                    "Only scalar channels have a numeric sampling observation",
                ));
            };
            Ok(KeyframeSampleV1 {
                time: time.clone(),
                value: raw,
                physical_value: physical,
            })
        })
        .collect::<Result<Vec<_>, EditorError>>()?;
    Ok(KeyframeSampleResultV1 {
        schema_version: 1,
        revision: input.revision.clone(),
        channel_id: input.channel_id.clone(),
        samples,
    })
}
