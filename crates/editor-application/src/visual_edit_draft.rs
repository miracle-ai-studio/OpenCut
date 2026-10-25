//! Read-only candidate admission over the original public edit planner.
use crate::{
    element_transform,
    scene::public::{ProjectPublicResources, plan_public},
};
use opencut_contracts::{EditorError, Time, project_public::*, scene::*, visual_edit_draft::*};
use sha2::{Digest, Sha256};

fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}

pub fn prepare(
    store: &impl ProjectPublicResources,
    transaction: &ApplyProject,
    sequence_id: &str,
    time: &Time,
    intent: VisualEditDraftIntent,
) -> Result<(EditorSnapshot, String), EditorError> {
    let snapshot = match intent {
        VisualEditDraftIntent::Gesture { gesture } => {
            element_transform::prepare_draft(store, transaction, sequence_id, time, gesture)?.0
        }
        VisualEditDraftIntent::Property { property } => {
            prepare_property(store, transaction, sequence_id, time, property)?
        }
    };
    // Bind the explicit admission semantics as well as the full transaction.
    let digest = format!(
        "sha256:{:x}",
        Sha256::digest(
            serde_json::to_vec(&(intent, transaction))
                .map_err(|_| invalid("Visual draft cannot be encoded"))?
        )
    );
    Ok((snapshot, digest))
}

fn prepare_property(
    store: &impl ProjectPublicResources,
    transaction: &ApplyProject,
    sequence_id: &str,
    time: &Time,
    property: VisualProperty,
) -> Result<EditorSnapshot, EditorError> {
    let ids = transaction
        .operations
        .iter()
        .map(|op| match op {
            ProjectEditOperation::ElementPut { element, .. } => Ok(element.id.as_str()),
            _ => Err(invalid(
                "Visual property drafts accept only existing ElementPut operations",
            )),
        })
        .collect::<Result<Vec<_>, _>>()?;
    element_transform::envelope(
        &transaction.project_id,
        &transaction.expected_revision,
        sequence_id,
        time,
        &ids,
    )?;
    let mut snapshot = element_transform::head(store, transaction)?;
    let sequence = element_transform::sequence(&snapshot.project, sequence_id)?;
    let parameter = match property {
        VisualProperty::PositionXPx => ScalarParameter::PositionXPx,
        VisualProperty::PositionYPx => ScalarParameter::PositionYPx,
        VisualProperty::ScaleX => ScalarParameter::ScaleX,
        VisualProperty::ScaleY => ScalarParameter::ScaleY,
        VisualProperty::RotationDegrees => ScalarParameter::RotationDegrees,
        VisualProperty::Opacity => ScalarParameter::Opacity,
    };
    for op in &transaction.operations {
        let ProjectEditOperation::ElementPut {
            sequence_id: owner,
            track_id,
            element,
        } = op
        else {
            unreachable!()
        };
        let (track, original) =
            element_transform::visual_target(sequence, &element.id, time, true)?;
        if owner != sequence_id || track_id != &track.id {
            return Err(invalid("Visual property draft contains a foreign owner"));
        }
        if original.channels.iter().any(|channel| {
            matches!(channel,
            AnimationChannel::Scalar { target: ChannelTarget::Element { element_id },
                binding: ScalarBinding::Property { parameter: bound }, keys, .. }
                if element_id == &element.id && *bound == parameter && !keys.is_empty())
        }) {
            return Err(invalid(
                "Edit this animated property with the keyframe controls",
            ));
        }
        let mut expected = original.clone();
        let before = expected.visual.as_mut().expect("validated visual target");
        let after = element
            .visual
            .as_ref()
            .ok_or_else(|| invalid("Visual draft has no visual properties"))?;
        match property {
            VisualProperty::PositionXPx => {
                before.transform.position_x_px = after.transform.position_x_px
            }
            VisualProperty::PositionYPx => {
                before.transform.position_y_px = after.transform.position_y_px
            }
            VisualProperty::ScaleX => before.transform.scale_x = after.transform.scale_x,
            VisualProperty::ScaleY => before.transform.scale_y = after.transform.scale_y,
            VisualProperty::RotationDegrees => {
                before.transform.rotation_degrees = after.transform.rotation_degrees
            }
            VisualProperty::Opacity => before.opacity = after.opacity,
        }
        if expected != **element {
            return Err(invalid(
                "Visual property draft may only change the requested static field; preserve all channels and other fields",
            ));
        }
    }
    plan_public(store, transaction)?;
    element_transform::fence(store, transaction)?;
    let sequence = snapshot
        .project
        .sequences
        .iter_mut()
        .find(|s| s.id == sequence_id)
        .expect("validated scene");
    for op in &transaction.operations {
        let ProjectEditOperation::ElementPut {
            track_id, element, ..
        } = op
        else {
            unreachable!()
        };
        let original = sequence
            .tracks
            .iter_mut()
            .find(|t| &t.id == track_id)
            .expect("validated track")
            .elements
            .iter_mut()
            .find(|e| e.id == element.id)
            .expect("validated element");
        *original = (**element).clone();
    }
    Ok(snapshot)
}
