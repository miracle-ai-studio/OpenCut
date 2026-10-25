//! Pure sampled gesture admission over the same fixed-head public planner as
//! V1. Only explicit scale intents can remove their exact element-axis channels.
use crate::scene::{
    SceneCommitIntent, StoredProjectDocument,
    public::{ProjectPublicResources, plan_public},
};
use opencut_composition::{CompositionScope, scene::compile_frame_scene};
use opencut_contracts::{EditorError, Time, element_transform::*, project_public::*, scene::*};
use opencut_time::RationalTime as R;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

mod geometry;
#[cfg(test)]
mod tests;

fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
fn bounded_id(id: &str) -> bool {
    !id.trim().is_empty() && id.len() <= 128 && !id.chars().any(char::is_control)
}
pub(crate) fn envelope(
    project: &str,
    revision: &str,
    sequence: &str,
    time: &Time,
    ids: &[&str],
) -> Result<(), EditorError> {
    if ![project, revision, sequence].into_iter().all(bounded_id)
        || ids.is_empty()
        || ids.len() > 60
        || ids.iter().any(|id| !bounded_id(id))
        || ids.iter().copied().collect::<BTreeSet<_>>().len() != ids.len()
    {
        return Err(invalid(
            "Transform gestures require fixed identities and 1..60 unique elements",
        ));
    }
    if R::try_from(time)?.ticks() < 0 {
        return Err(invalid("Transform gestures require nonnegative exact time"));
    }
    Ok(())
}
pub(crate) fn head(
    store: &impl ProjectPublicResources,
    transaction: &ApplyProject,
) -> Result<EditorSnapshot, EditorError> {
    store.authorize_project(SceneCommitIntent::PublicApply(transaction))?;
    let source = store.head_scene_source(&transaction.project_id)?;
    if source.document.project_id() != transaction.project_id {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Transform head scope differs",
        ));
    }
    if source.revision != transaction.expected_revision {
        return Err(crate::revision_conflict(
            &transaction.expected_revision,
            &source.revision,
        ));
    }
    let StoredProjectDocument::Editable(project) = source.document else {
        return Err(EditorError::new(
            "UNSUPPORTED_PROJECT_VERSION",
            "Transform gestures require a V4 head",
        ));
    };
    opencut_editor_core::scene::validate_project_scene(&project)?;
    Ok(EditorSnapshot {
        project: *project,
        revision: source.revision,
    })
}
pub(crate) fn fence(
    store: &impl ProjectPublicResources,
    transaction: &ApplyProject,
) -> Result<(), EditorError> {
    store.authorize_project(SceneCommitIntent::PublicApply(transaction))?;
    let current = store.head_scene_source(&transaction.project_id)?;
    if current.document.project_id() != transaction.project_id {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Transform head scope differs",
        ));
    }
    if current.revision != transaction.expected_revision {
        return Err(crate::revision_conflict(
            &transaction.expected_revision,
            &current.revision,
        ));
    }
    Ok(())
}
pub(crate) fn sequence<'a>(project: &'a EditorDocument, id: &str) -> Result<&'a Scene, EditorError> {
    project
        .sequences
        .iter()
        .find(|s| s.id == id)
        .ok_or_else(|| invalid("Transform scene does not exist"))
}
fn target<'a>(
    sequence: &'a Scene,
    id: &str,
    time: &Time,
) -> Result<(&'a SceneTrack, &'a TimelineElement), EditorError> {
    visual_target(sequence, id, time, false)
}
/// Property edits compose with transitions in the existing IR. Gesture geometry
/// retains its original stricter transition admission.
pub(crate) fn visual_target<'a>(
    sequence: &'a Scene,
    id: &str,
    time: &Time,
    allow_transitions: bool,
) -> Result<(&'a SceneTrack, &'a TimelineElement), EditorError> {
    let (track, element) = sequence
        .tracks
        .iter()
        .find_map(|track| {
            track
                .elements
                .iter()
                .find(|e| e.id == id)
                .map(|element| (track, element))
        })
        .ok_or_else(|| invalid("Transform element does not exist in this scene"))?;
    if track.locked {
        return Err(EditorError::new(
            "TRACK_LOCKED",
            "Unlock the transform track",
        ));
    }
    let start = R::try_from(&element.timeline_range.start)?;
    let end = start.checked_add(R::try_from(&element.timeline_range.duration)?)?;
    let at = R::try_from(time)?;
    if !track.enabled
        || !element.enabled
        || at.cmp_exact(start).is_lt()
        || !at.cmp_exact(end).is_lt()
    {
        return Err(invalid("Transform element is inactive at the paused time"));
    }
    if !allow_transitions && sequence
        .transitions
        .iter()
        .any(|t| t.outgoing_element_id == element.id || t.incoming_element_id == element.id)
    {
        return Err(invalid(
            "Transform gestures do not edit transitioning elements",
        ));
    }
    if !matches!(
        element.content,
        ElementContent::Video { .. }
            | ElementContent::Image { .. }
            | ElementContent::Graphic { .. }
            | ElementContent::Sticker { .. }
            | ElementContent::Text { .. }
    ) || element.visual.as_ref().is_none_or(|visual| {
        visual.profile != VisualProfile::ClassicCanvasV31
            && !(visual.profile == VisualProfile::LegacyCompositionV3
                && matches!(
                    element.content,
                    ElementContent::Video { .. } | ElementContent::Image { .. }
                ))
    }) {
        return Err(invalid(
            "Transform editing requires supported visual geometry",
        ));
    }
    Ok((track, element))
}
fn parameter(channel: &AnimationChannel, element_id: &str) -> Option<ScalarParameter> {
    match channel {
        AnimationChannel::Scalar {
            target: ChannelTarget::Element { element_id: id },
            binding: ScalarBinding::Property { parameter },
            ..
        } if id == element_id => Some(*parameter),
        _ => None,
    }
}
fn has_position_animation(element: &TimelineElement) -> bool {
    element.channels.iter().any(|channel| {
        matches!(
            parameter(channel, &element.id),
            Some(ScalarParameter::PositionXPx | ScalarParameter::PositionYPx)
        )
    })
}
fn removed_axis(parameter: Option<ScalarParameter>, intent: TransformGestureIntent) -> bool {
    matches!(
        (parameter, intent),
        (
            Some(ScalarParameter::ScaleX),
            TransformGestureIntent::ScaleX | TransformGestureIntent::ScaleXy
        ) | (
            Some(ScalarParameter::ScaleY),
            TransformGestureIntent::ScaleY | TransformGestureIntent::ScaleXy
        )
    )
}

pub fn sample(
    store: &impl ProjectPublicResources,
    input: &SampleElementTransforms,
) -> Result<SampledElementTransforms, EditorError> {
    envelope(
        &input.project_id,
        &input.expected_revision,
        &input.sequence_id,
        &input.time,
        &input
            .element_ids
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
    )?;
    let authority = ApplyProject {
        branch_name: None,
        project_id: input.project_id.clone(),
        expected_revision: input.expected_revision.clone(),
        idempotency_key: "transform-sample".into(),
        label: "Sample transforms".into(),
        operations: vec![],
    };
    let snapshot = head(store, &authority)?;
    let scene = sequence(&snapshot.project, &input.sequence_id)?;
    for id in &input.element_ids {
        target(scene, id, &input.time)?;
    }
    // No media/font reader is involved. The shared pure compiler applies exact
    // local time, extrapolation, tangent evaluation and physical parameter bounds.
    let graph = compile_frame_scene(
        &snapshot,
        &input.sequence_id,
        &input.time,
        CompositionScope::Visual,
    )
    .map_err(|error| EditorError::new(error.code, error.message))?;
    let elements = input
        .element_ids
        .iter()
        .map(|id| {
            let layer = graph
                .layers
                .iter()
                .find(|layer| &layer.element_id == id)
                .ok_or_else(|| invalid("Requested transform was not emitted at this time"))?;
            let transform = layer
                .visual
                .as_ref()
                .ok_or_else(|| invalid("Sampled layer has no transform"))?
                .transform;
            Ok(SampledElementTransform {
                element_id: id.clone(),
                track_id: layer.track_id.clone(),
                transform,
                geometry: geometry::sampled_geometry(&graph, layer)?,
            })
        })
        .collect::<Result<Vec<_>, EditorError>>()?;
    fence(store, &authority)?;
    Ok(SampledElementTransforms {
        project_id: input.project_id.clone(),
        revision: input.expected_revision.clone(),
        sequence_id: input.sequence_id.clone(),
        time: input.time.clone(),
        elements,
    })
}

pub fn prepare_draft(
    store: &impl ProjectPublicResources,
    transaction: &ApplyProject,
    sequence_id: &str,
    time: &Time,
    intent: TransformGestureIntent,
) -> Result<(EditorSnapshot, String), EditorError> {
    let ids = transaction
        .operations
        .iter()
        .map(|operation| match operation {
            ProjectEditOperation::ElementPut { element, .. } => Ok(element.id.as_str()),
            _ => Err(invalid(
                "Transform gestures accept only existing ElementPut operations",
            )),
        })
        .collect::<Result<Vec<_>, EditorError>>()?;
    envelope(
        &transaction.project_id,
        &transaction.expected_revision,
        sequence_id,
        time,
        &ids,
    )?;
    let snapshot = head(store, transaction)?;
    let scene = sequence(&snapshot.project, sequence_id)?;
    for operation in &transaction.operations {
        let ProjectEditOperation::ElementPut {
            sequence_id: owner,
            track_id,
            element,
        } = operation
        else {
            unreachable!()
        };
        let (track, original) = target(scene, &element.id, time)?;
        if owner != sequence_id || track_id != &track.id {
            return Err(invalid("Transform gesture contains a foreign owner"));
        }
        let before = original.visual.as_ref().expect("target visual").transform;
        let after = element
            .visual
            .as_ref()
            .ok_or_else(|| invalid("Transform gesture has no visual transform"))?
            .transform;
        let moved = before.position_x_px != after.position_x_px
            || before.position_y_px != after.position_y_px;
        if (moved || intent == TransformGestureIntent::Translate)
            && has_position_animation(original)
        {
            return Err(invalid(
                "Position animation cannot be overwritten by a translation or group pivot",
            ));
        }
        if intent == TransformGestureIntent::Rotate
            && original.channels.iter().any(|channel| {
                parameter(channel, &original.id) == Some(ScalarParameter::RotationDegrees)
            })
        {
            return Err(invalid(
                "Rotation animation cannot be overwritten by a rotation gesture",
            ));
        }
        let mut expected = original.clone();
        let transform = &mut expected.visual.as_mut().expect("target visual").transform;
        // All gestures may change position for an anchor/group pivot, subject
        // to the position-animation guard. Unrelated authored axes stay exact.
        transform.position_x_px = after.position_x_px;
        transform.position_y_px = after.position_y_px;
        match intent {
            TransformGestureIntent::Translate => {}
            TransformGestureIntent::Rotate => transform.rotation_degrees = after.rotation_degrees,
            TransformGestureIntent::ScaleX => transform.scale_x = after.scale_x,
            TransformGestureIntent::ScaleY => transform.scale_y = after.scale_y,
            TransformGestureIntent::ScaleXy => {
                transform.scale_x = after.scale_x;
                transform.scale_y = after.scale_y;
            }
        }
        expected
            .channels
            .retain(|channel| !removed_axis(parameter(channel, &original.id), intent));
        if expected != **element {
            return Err(invalid(
                "Transform gesture may only alter its requested axes and remove their exact scale channels",
            ));
        }
    }
    // Same public planner and aggregate/lock rules as the eventual single commit.
    // This is an ephemeral proposal: no receipt, CAS access, journal or revision.
    plan_public(store, transaction)?;
    fence(store, transaction)?;
    let mut project = snapshot.project;
    let sequence = project
        .sequences
        .iter_mut()
        .find(|scene| scene.id == sequence_id)
        .expect("validated scene");
    for operation in &transaction.operations {
        let ProjectEditOperation::ElementPut {
            track_id, element, ..
        } = operation
        else {
            unreachable!()
        };
        let track = sequence
            .tracks
            .iter_mut()
            .find(|track| &track.id == track_id)
            .expect("validated owner");
        let original = track
            .elements
            .iter_mut()
            .find(|original| original.id == element.id)
            .expect("validated target");
        *original = (**element).clone();
    }
    let digest = format!(
        "sha256:{:x}",
        Sha256::digest(
            serde_json::to_vec(&(intent, transaction))
                .map_err(|_| invalid("Transform gesture cannot be encoded"))?
        )
    );
    Ok((
        EditorSnapshot {
            project,
            revision: snapshot.revision,
        },
        digest,
    ))
}
