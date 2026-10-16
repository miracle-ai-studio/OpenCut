//! Fixed-head conversion review, using the existing keyframe authoring/public
//! transaction path. No write, resource discovery or native sampler is added.
use crate::project_v4::{StoredProjectDocumentV4, V4Intent, public::ProjectV4PublicResources};
use opencut_contracts::{
    EditorError, keyframe_edit_v1::*, keyframe_legacy_conversion_v1::*, project_v4::*,
    project_v4_public::ApplyPublicV4Project,
};
use opencut_editor_core::project_v4::{
    LEGACY_LOOKUP_CONVERSION_PROFILE_V1, convert_legacy_lookup_segments_v1,
};
use std::collections::HashSet;

fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
fn channel_id(channel: &AnimationChannelV4) -> &str {
    match channel {
        AnimationChannelV4::Scalar { id, .. } | AnimationChannelV4::Discrete { id, .. } => id,
    }
}
fn reserve(value: &serde_json::Value, ids: &mut HashSet<String>) {
    match value {
        serde_json::Value::Object(fields) => {
            for (key, value) in fields {
                if matches!(key.as_str(), "id" | "segment_id") {
                    if let Some(id) = value.as_str() {
                        ids.insert(id.into());
                    }
                }
                reserve(value, ids);
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                reserve(value, ids)
            }
        }
        _ => {}
    }
}
pub fn plan(
    store: &impl ProjectV4PublicResources,
    input: &KeyframeLegacyConversionRequestV1,
) -> Result<KeyframeLegacyConversionResultV1, EditorError> {
    for id in [&input.project_id, &input.sequence_id, &input.channel_id] {
        if id.trim().is_empty() || id.len() > 128 || id.chars().any(char::is_control) {
            return Err(invalid("Conversion scope has an invalid identity"));
        }
    }
    let intent = ApplyPublicV4Project {
        project_id: input.project_id.clone(),
        expected_revision: input.expected_revision.clone(),
        idempotency_key: input.idempotency_key.clone(),
        label: input.label.clone(),
        operations: vec![],
    };
    store.authorize_v4(V4Intent::PublicApply(&intent))?;
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
            "Conversion requires V4",
        ));
    };
    if project.id != input.project_id {
        return Err(invalid("Conversion project scope differs"));
    }
    opencut_editor_core::project_v4::validate_project_v4(&project)?;
    let sequence = project
        .sequences
        .iter()
        .find(|s| s.id == input.sequence_id)
        .ok_or_else(|| EditorError::new("NOT_FOUND", "Conversion sequence does not exist"))?;
    let mut found = None;
    for track in &sequence.tracks {
        for (owner, channels) in std::iter::once((
            KeyframeOwnerV1::Track {
                track_id: track.id.clone(),
            },
            &track.channels,
        ))
        .chain(track.elements.iter().map(|e| {
            (
                KeyframeOwnerV1::Element {
                    element_id: e.id.clone(),
                },
                &e.channels,
            )
        })) {
            if let Some(channel) = channels.iter().find(|c| channel_id(c) == input.channel_id) {
                if track.locked {
                    return Err(EditorError::new(
                        "TRACK_LOCKED",
                        "Unlock the legacy keyframe track",
                    ));
                }
                found = Some((owner, channel));
                break;
            }
        }
        if found.is_some() {
            break;
        }
    }
    let (owner, channel) =
        found.ok_or_else(|| EditorError::new("NOT_FOUND", "Conversion channel does not exist"))?;
    let mut ids = HashSet::new();
    reserve(
        &serde_json::to_value(&project)
            .map_err(|_| invalid("Cannot reserve project identities"))?,
        &mut ids,
    );
    let converted = convert_legacy_lookup_segments_v1(
        channel,
        &input.left_key_ids,
        &ids,
        input.maximum_absolute_error,
    )?;
    // This repeats the fixed-head/auth fence in the existing planner. A head
    // change between discovery and planning rejects rather than changing input.
    let plan = crate::keyframe_edit_v1::plan(
        store,
        &KeyframeEditRequestV1 {
            project_id: input.project_id.clone(),
            expected_revision: input.expected_revision.clone(),
            idempotency_key: input.idempotency_key.clone(),
            label: input.label.clone(),
            sequence_id: input.sequence_id.clone(),
            edits: vec![KeyframeEditV1::ChannelPut {
                owner,
                channel: converted.channel,
            }],
        },
    )?;
    Ok(KeyframeLegacyConversionResultV1 {
        plan,
        profile: LEGACY_LOOKUP_CONVERSION_PROFILE_V1.into(),
        converted_segments: converted.converted_segments,
        inserted_keys: converted.inserted_keys,
        max_absolute_error: converted.max_absolute_error,
        error_bound: converted.error_bound,
    })
}
