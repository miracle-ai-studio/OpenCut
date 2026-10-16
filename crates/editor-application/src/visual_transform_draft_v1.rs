//! A bounded transform-only proposal on the authorized original head. The
//! existing public planner validates locks and the aggregate; no store commits.
use crate::project_v4::{
    StoredProjectDocumentV4, V4Intent,
    public::{ProjectV4PublicResources, plan_public},
};
use opencut_contracts::{EditorError, Time, project_v4::*, project_v4_public::*};
use opencut_time::RationalTime as R;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
pub fn prepare(
    store: &impl ProjectV4PublicResources,
    transaction: &ApplyPublicV4Project,
    sequence_id: &str,
    time: &Time,
) -> Result<(ProjectSnapshotV4, String), EditorError> {
    store.authorize_v4(V4Intent::PublicApply(transaction))?;
    if transaction.operations.is_empty() || transaction.operations.len() > 128 {
        return Err(invalid("Choose 1..128 transform-only elements"));
    }
    let source = store.head_v4_source(&transaction.project_id)?;
    if source.revision != transaction.expected_revision {
        return Err(crate::revision_conflict(
            &transaction.expected_revision,
            &source.revision,
        ));
    }
    let StoredProjectDocumentV4::V4(project) = source.document else {
        return Err(invalid("Transform drafts require a V4 head"));
    };
    if project.id != transaction.project_id {
        return Err(invalid("Transform draft project differs"));
    }
    let mut project = *project;
    let sequence = project
        .sequences
        .iter_mut()
        .find(|s| s.id == sequence_id)
        .ok_or_else(|| invalid("Transform draft scene does not exist"))?;
    let at = R::try_from(time)?;
    let transitioning: BTreeSet<_> = sequence
        .transitions
        .iter()
        .flat_map(|t| [t.outgoing_element_id.clone(), t.incoming_element_id.clone()])
        .collect();
    let mut ids = BTreeSet::new();
    for operation in &transaction.operations {
        let PublicProjectOperationV4::ElementPut {
            sequence_id: owner,
            track_id,
            element,
        } = operation
        else {
            return Err(invalid(
                "Transform drafts accept only existing element transforms",
            ));
        };
        if owner != sequence_id || !ids.insert(element.id.clone()) {
            return Err(invalid(
                "Transform draft contains duplicate or foreign elements",
            ));
        }
        let track = sequence
            .tracks
            .iter_mut()
            .find(|t| t.id == *track_id)
            .ok_or_else(|| invalid("Transform draft track does not exist"))?;
        if track.locked {
            return Err(EditorError::new(
                "TRACK_LOCKED",
                "Unlock the transform track",
            ));
        }
        if !track.enabled {
            return Err(invalid("Transform draft track is disabled"));
        }
        let original = track
            .elements
            .iter_mut()
            .find(|e| e.id == element.id)
            .ok_or_else(|| invalid("Transform draft element does not exist"))?;
        if !original.enabled
            || opencut_editor_core::project_v4::has_element_transform_animation_v1(original)
            || transitioning.contains(&original.id)
            || !matches!(
                &original.content,
                ElementContentV4::Video { .. }
                    | ElementContentV4::Image { .. }
                    | ElementContentV4::Graphic { .. }
                    | ElementContentV4::Sticker { .. }
                    | ElementContentV4::Text { .. }
            )
        {
            return Err(invalid(
                "Transform draft requires active visual elements without transform animation or transitions",
            ));
        }
        let start = R::try_from(&original.timeline_range.start)?;
        let end = start.checked_add(R::try_from(&original.timeline_range.duration)?)?;
        if at.cmp_exact(start).is_lt() || !at.cmp_exact(end).is_lt() {
            return Err(invalid(
                "Transform draft element is inactive at the paused time",
            ));
        }
        let transform = element
            .visual
            .as_ref()
            .ok_or_else(|| invalid("Transform draft has no visual transform"))?
            .transform;
        let mut expected = original.clone();
        expected
            .visual
            .as_mut()
            .ok_or_else(|| invalid("Original element has no visual transform"))?
            .transform = transform;
        if expected != **element {
            return Err(invalid(
                "Transform draft cannot alter content, resources, channels or other properties",
            ));
        }
        *original = expected;
    }
    // Authorization and source resolution are the same as the eventual public
    // transaction. The returned candidate remains an ephemeral fixed-head view.
    plan_public(store, transaction)?;
    store.authorize_v4(V4Intent::PublicApply(transaction))?;
    let digest = format!(
        "sha256:{:x}",
        Sha256::digest(
            serde_json::to_vec(transaction)
                .map_err(|_| invalid("Transform draft cannot be encoded"))?
        )
    );
    Ok((
        ProjectSnapshotV4 {
            project,
            revision: source.revision,
        },
        digest,
    ))
}
