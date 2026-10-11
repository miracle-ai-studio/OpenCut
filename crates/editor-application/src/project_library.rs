//! Library projections and copy validation; no persistence or media I/O.
use opencut_api_contracts::project_library::{
    CopyProject, DeleteOutcome, DeleteProjects, DeleteReceipt, DuplicateProjects,
};
use opencut_contracts::{
    EditorError, ProjectDocument, Time, TimeRange, project_v3::ProjectDocumentV3,
};
use opencut_time::RationalTime;
use std::collections::HashSet;

pub trait ProjectDeletionStore: crate::uploads::UploadRetentionStore {
    fn delete_projects_atomic(
        &mut self,
        input: &DeleteProjects,
    ) -> Result<DeleteReceipt, EditorError>;
}
pub fn delete_projects(
    store: &mut impl ProjectDeletionStore,
    cleanup: &impl crate::uploads::UploadCleanup,
    input: &DeleteProjects,
) -> Result<DeleteOutcome, EditorError> {
    validate_delete(input)?;
    let receipt = store.delete_projects_atomic(input)?;
    let mut cleanup_pending = Vec::new();
    for id in &receipt.staging_upload_ids {
        if crate::uploads::release_upload(
            store,
            cleanup,
            opencut_api_contracts::uploads::UploadInput {
                upload_id: id.clone(),
            },
        )
        .is_err()
        {
            cleanup_pending.push(id.clone());
        }
    }
    Ok(DeleteOutcome {
        receipt,
        cleanup_pending,
    })
}

pub fn validate_delete(input: &DeleteProjects) -> Result<(), EditorError> {
    let mut ids = HashSet::new();
    if input.idempotency_key.trim().is_empty()
        || input.projects.is_empty()
        || input.projects.len() > 64
        || input.projects.iter().any(|p| {
            p.project_id.trim().is_empty()
                || p.expected_revision.trim().is_empty()
                || !ids.insert(&p.project_id)
        })
    {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Deletion requires 1..64 unique projects, fixed revisions and an idempotency key",
        ));
    }
    Ok(())
}

pub fn validate(input: &DuplicateProjects) -> Result<(), EditorError> {
    let mut targets = HashSet::new();
    if input.idempotency_key.trim().is_empty()
        || input.projects.is_empty()
        || input.projects.len() > 64
    {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "A batch of 1 to 64 projects and an idempotency key are required",
        ));
    }
    for item in &input.projects {
        if item.name.trim().is_empty()
            || item.target_project_id.trim().is_empty()
            || item.source_project_id.trim().is_empty()
            || item.expected_revision.trim().is_empty()
            || item.target_project_id == item.source_project_id
            || !targets.insert(&item.target_project_id)
        {
            return Err(EditorError::new(
                "INVALID_ARGUMENT",
                "Each copy needs a name, fixed source revision and unique new target ID",
            ));
        }
    }
    Ok(())
}
pub fn copy_base(
    source: &ProjectDocument,
    input: &CopyProject,
) -> Result<ProjectDocument, EditorError> {
    if source.id != input.source_project_id {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Source project identity differs from scope",
        ));
    }
    let mut result = source.clone();
    result.id = input.target_project_id.clone();
    result.name = input.name.trim().into();
    opencut_editor_core::validate_project(&result)?;
    Ok(result)
}
pub fn copy_visual(
    source: &ProjectDocumentV3,
    input: &CopyProject,
) -> Result<ProjectDocumentV3, EditorError> {
    let mut result = source.clone();
    result.base = copy_base(&source.base, input)?;
    opencut_editor_core::project_v3::validate_project_v3(&result)?;
    Ok(result)
}
pub fn duration(
    base: &ProjectDocument,
    visual: Option<&ProjectDocumentV3>,
) -> Result<Time, EditorError> {
    let main = base
        .sequences
        .iter()
        .find(|s| s.id == base.default_sequence_id)
        .ok_or_else(|| EditorError::new("INVALID_PROJECT", "Main scene is missing"))?;
    let mut end = RationalTime::new(0, 1)?;
    let mut visit = |range: &TimeRange| -> Result<(), EditorError> {
        let next = RationalTime::try_from(&range.start)?
            .checked_add(RationalTime::try_from(&range.duration)?)?;
        if next.cmp_exact(end).is_gt() {
            end = next;
        }
        Ok(())
    };
    for track in &main.tracks {
        for clip in &track.clips {
            visit(&clip.timeline_range)?;
        }
    }
    for cue in &main.caption_cues {
        visit(&cue.timeline_range)?;
    }
    if let Some(visual) = visual {
        for scene in &visual.visual {
            if scene.sequence_id == main.id {
                for shape in &scene.shapes {
                    visit(&shape.timeline_range)?;
                }
            }
        }
    }
    Ok(end.into())
}

pub fn base_revision(document: &ProjectDocument) -> Result<String, EditorError> {
    Ok(super::content_id(
        "rev",
        super::encode_json(document, "copied project")?.as_bytes(),
    ))
}
