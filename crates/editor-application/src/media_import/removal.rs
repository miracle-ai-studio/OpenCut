//! Fixed-head logical media removal. Reuse the public atomic edit, locks and
//! resource fences; never delete a CAS resource or rewrite stored history.
use super::*;
use crate::scene::public::plan_public;

pub fn plan_removal(
    store: &impl ProjectPublicResources,
    input: &MediaRemovalRequest,
) -> Result<ApplyProject, EditorError> {
    crate::require_idempotency_key(&input.idempotency_key)?;
    let bounded_id =
        |id: &str| !id.trim().is_empty() && id.len() <= 128 && !id.chars().any(char::is_control);
    let ids: BTreeSet<_> = input.asset_ids.iter().map(String::as_str).collect();
    if !bounded_id(&input.project_id)
        || !bounded_id(&input.expected_revision)
        || ids.is_empty()
        || ids.len() > 60
        || ids.len() != input.asset_ids.len()
        || !ids.iter().all(|id| bounded_id(id))
        || input.label.trim().is_empty()
        || input.label.len() > 4096
        || input.label.chars().any(char::is_control)
    {
        return Err(invalid(
            "Media removal needs fixed identities, 1..60 distinct assets and a label",
        ));
    }
    let mut tx = ApplyProject {
        branch_name: None,
        project_id: input.project_id.clone(),
        expected_revision: input.expected_revision.clone(),
        idempotency_key: input.idempotency_key.clone(),
        label: input.label.clone(),
        operations: vec![],
    };
    store.authorize_project(SceneCommitIntent::PublicApply(&tx))?;
    let source = store.head_scene_source(&input.project_id)?;
    if source.document.project_id() != input.project_id {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Media removal head scope differs",
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
            "Media removal requires a V4 head",
        ));
    };
    opencut_editor_core::scene::validate_project_scene(&project)?;
    if ids
        .iter()
        .any(|id| !project.assets.iter().any(|a| a.id == *id))
    {
        return Err(missing(
            "A media removal asset is not bound to this project",
        ));
    }
    let mut expected = (*project).clone();
    for sequence in &mut expected.sequences {
        let before = sequence.clone();
        let mut removed = BTreeSet::new();
        for track in &mut sequence.tracks {
            track.elements.retain(|element| {
                let asset = match &element.content {
                    ElementContent::Video { asset_id, .. }
                    | ElementContent::Audio { asset_id, .. }
                    | ElementContent::Image { asset_id }
                    | ElementContent::Sticker { asset_id, .. } => Some(asset_id.as_str()),
                    _ => None,
                };
                let keep = !asset.is_some_and(|id| ids.contains(id));
                if !keep {
                    removed.insert(element.id.clone());
                }
                keep
            });
        }
        sequence.transitions.retain(|t| {
            !removed.contains(&t.outgoing_element_id) && !removed.contains(&t.incoming_element_id)
        });
        // Caption text/style/timing survive removal of their provenance media.
        for caption in &mut sequence.captions {
            if caption
                .source_anchor
                .as_ref()
                .is_some_and(|a| ids.contains(a.asset_id.as_str()))
            {
                caption.source_anchor = None;
            }
        }
        if *sequence != before {
            tx.operations.push(ProjectEditOperation::SequencePut {
                sequence: sequence.clone(),
            });
        }
    }
    expected.assets.retain(|a| !ids.contains(a.id.as_str()));
    tx.operations.extend(
        input
            .asset_ids
            .iter()
            .map(|id| ProjectEditOperation::AssetDelete {
                asset_id: id.clone(),
            }),
    );
    // Parent replacement passes the existing locked-element/track/caption and
    // bookmark checks. Validate the complete result before returning a proposal.
    let proposed = plan_public(store, &tx)?;
    let public = crate::scene::public::project(&EditorSnapshot {
        project: expected,
        revision: source.revision,
    });
    if proposed != public.project {
        return Err(EditorError::new(
            "INVALID_MEDIA_REMOVAL_PLAN",
            "Media removal changed unrelated content",
        ));
    }
    store.authorize_project(SceneCommitIntent::PublicApply(&tx))?;
    Ok(tx)
}
