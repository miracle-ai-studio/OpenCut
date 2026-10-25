use super::*;
use serde_json::json;

pub fn copy(
    store: &mut impl SceneStore,
    request: &CopySceneProject,
) -> Result<EditorSnapshot, EditorError> {
    require_idempotency_key(&request.idempotency_key)?;
    store.authorize_project(SceneCommitIntent::Copy(request))?;
    if let Some(result) = store.replay_scene_copy(request)? {
        return Ok(result);
    }
    // Copy is a V4 aggregate operation, not an implicit legacy migration.
    let source = inspect(
        store,
        &InspectProject {
            project_id: request.source_project_id.clone(),
            revision: Some(request.source_revision.clone()),
        },
    )?;
    let mut project = source.project;
    project.id = request.project_id.clone();
    project.name = request.name.clone();
    validate_project_scene(&project)?;
    let revision = content_id(
        "rev",
        encode_json(&project, "V4 copied project")?.as_bytes(),
    );
    store.copy_scene_atomic(
        request,
        PreparedCreate {
            snapshot: EditorSnapshot { project, revision },
            event_id: format!("evt_{}", Uuid::new_v4().simple()),
            request_json: SceneCommitIntent::Copy(request).request_json()?,
        },
    )
}

pub fn create(
    store: &mut impl SceneStore,
    request: &CreateProjectRequest,
) -> Result<EditorSnapshot, EditorError> {
    require_idempotency_key(&request.idempotency_key)?;
    store.authorize_project(SceneCommitIntent::Create(request))?;
    if let Some(result) = store.replay_scene_create(request)? {
        return Ok(result);
    }
    match store.head_scene_source(&request.project.id) {
        Ok(_) => {
            if let Some(result) = store.replay_scene_create(request)? {
                return Ok(result);
            }
            return Err(EditorError::new("ALREADY_EXISTS", "Project already exists"));
        }
        Err(error) if error.code == "NOT_FOUND" => {}
        Err(error) => return Err(error),
    }
    let project = new_document(&request.project)?;
    let revision = content_id("rev", encode_json(&project, "V4 project")?.as_bytes());
    store.create_scene_atomic(
        request,
        PreparedCreate {
            snapshot: EditorSnapshot { project, revision },
            event_id: format!("evt_{}", Uuid::new_v4().simple()),
            request_json: SceneCommitIntent::Create(request).request_json()?,
        },
    )
}

fn scoped(
    source: StoredProjectRevision,
    project_id: &str,
) -> Result<StoredProjectRevision, EditorError> {
    if source.document.project_id() != project_id || source.revision.trim().is_empty() {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Stored revision differs from requested project scope",
        ));
    }
    Ok(source)
}
pub fn inspect(
    store: &impl SceneRead,
    request: &InspectProject,
) -> Result<EditorSnapshot, EditorError> {
    let source = match request.revision.as_deref() {
        Some(revision) => {
            let source = store.revision_scene_source(&request.project_id, revision)?;
            if source.revision != revision {
                return Err(EditorError::new(
                    "STORAGE_ERROR",
                    "Fixed revision read changed revision",
                ));
            }
            source
        }
        None => store.head_scene_source(&request.project_id)?,
    };
    let source = scoped(source, &request.project_id)?;
    let StoredProjectDocument::Editable(project) = source.document else {
        return Err(EditorError::new(
            "UNSUPPORTED_PROJECT_VERSION",
            "V4 inspection requires a promoted revision",
        ));
    };
    validate_project_scene(&project)?;
    Ok(EditorSnapshot {
        project: *project,
        revision: source.revision,
    })
}

pub(crate) fn checked_head(
    store: &impl SceneRead,
    intent: SceneCommitIntent<'_>,
) -> Result<StoredProjectRevision, EditorError> {
    let expected = intent
        .expected_revision()
        .ok_or_else(|| invalid("Edit requires expected revision"))?;
    if expected.trim().is_empty() {
        return Err(invalid("Expected revision must be nonempty"));
    }
    let source = scoped(
        store.head_scene_branch(intent.project_id(), intent.target_branch())?,
        intent.project_id(),
    )?;
    if source.revision != expected {
        return Err(revision_conflict(expected, &source.revision));
    }
    Ok(source)
}
pub(crate) fn begin(
    store: &impl SceneRead,
    intent: SceneCommitIntent<'_>,
) -> Result<Result<StoredProjectRevision, TransactionResult>, EditorError> {
    require_idempotency_key(intent.key())?;
    store.authorize_project(intent)?;
    if let Some(result) = store.replay_scene_edit(intent)? {
        return Ok(Err(result));
    }
    match checked_head(store, intent) {
        Ok(source) => Ok(Ok(source)),
        Err(error) if error.code == "REVISION_CONFLICT" => {
            if let Some(result) = store.replay_scene_edit(intent)? {
                return Ok(Err(result));
            }
            Err(error)
        }
        Err(error) => Err(error),
    }
}
pub fn promote(
    store: &mut impl SceneStore,
    request: &PromoteProject,
) -> Result<TransactionResult, EditorError> {
    let intent = SceneCommitIntent::Promote(request);
    let source = match begin(store, intent)? {
        Ok(source) => source,
        Err(result) => return Ok(result),
    };
    if matches!(source.document, StoredProjectDocument::Editable(_)) {
        return Err(invalid("Project is already V4"));
    }
    let proposed = converted(&source.document, &request.asset_versions)?;
    store.commit_scene_atomic(intent, prepare(&source, proposed, intent)?)
}
pub fn apply(
    store: &mut impl SceneStore,
    request: &ApplySceneProject,
) -> Result<TransactionResult, EditorError> {
    let intent = SceneCommitIntent::Apply(request);
    let source = match begin(store, intent)? {
        Ok(source) => source,
        Err(result) => return Ok(result),
    };
    let StoredProjectDocument::Editable(project) = &source.document else {
        return Err(EditorError::new(
            "UNSUPPORTED_PROJECT_VERSION",
            "Promote the project before applying V4 edits",
        ));
    };
    let snapshot = EditorSnapshot {
        project: (**project).clone(),
        revision: source.revision.clone(),
    };
    let proposed = plan(&snapshot, request)?;
    store.commit_scene_atomic(intent, prepare(&source, proposed, intent)?)
}

/// One original-request receipt and one fixed-head font edit. Acquisition is
/// skipped on an authorized replay; downloaded content cannot re-plan a newer
/// project head. The native publisher retains CAS protection through commit.
pub fn import_font_resource(
    store: &mut impl SceneStore,
    fetcher: &impl crate::font_import::catalog::FontSourceFetch,
    inspector: &impl crate::font_import::FontInspector,
    publisher: &impl crate::font_import::FontCasPublish,
    request: &opencut_contracts::font_import::ImportFont,
    fetch_policy: &crate::font_import::catalog::FontFetchPolicy,
    inspection_policy: &crate::font_import::FontImportPolicy,
    checkpoint: &mut dyn crate::font_import::FontImportCheckpoint,
) -> Result<TransactionResult, EditorError> {
    use crate::font_import::{catalog::prepare_catalog_font, *};
    let intent = SceneCommitIntent::FontImport(request);
    let source = match begin(store, intent)? {
        Ok(source) => source,
        Err(result) => return Ok(result),
    };
    checkpoint.check()?;
    let font = expected_import_resource(request)?;
    let StoredProjectDocument::Editable(project) = &source.document else {
        return Err(EditorError::new(
            "UNSUPPORTED_PROJECT_VERSION",
            "Promote the project before importing fonts",
        ));
    };
    // This internal edit is a pure planner input only. Its family/key is never
    // persisted; receipt and journal remain the exact original import request.
    let operation = ApplySceneProject {
        project_id: request.project_id.clone(),
        expected_revision: request.expected_revision.clone(),
        idempotency_key: request.idempotency_key.clone(),
        label: format!("Import {}", font.family),
        operations: vec![ProjectOperation::FontPut { font: font.clone() }],
    };
    let proposed = plan(
        &EditorSnapshot {
            project: (**project).clone(),
            revision: source.revision.clone(),
        },
        &operation,
    )?;
    let commit = prepare(&source, proposed, intent)?;
    let acquired = prepare_catalog_font(
        fetcher,
        inspector,
        &request.catalog_id,
        request.font_id.clone(),
        request.required_text.clone(),
        fetch_policy,
        inspection_policy,
        checkpoint,
    )?;
    if acquired.font.resource != font {
        return Err(EditorError::new(
            "FONT_SOURCE_INTEGRITY_ERROR",
            "Inspected font differs from its admitted catalog resource",
        ));
    }
    checkpoint.check()?;
    let _publication = publisher.publish(&acquired.font, &acquired.bytes, checkpoint)?;
    checkpoint.check()?;
    store.commit_scene_atomic(intent, commit)
}

pub fn import_derived_font(
    store: &mut impl SceneStore,
    fetcher: &impl crate::font_derived::DerivedFontSource,
    inspector: &impl crate::font_import::FontInspector,
    publisher: &impl crate::font_import::FontCasPublish,
    request: &opencut_contracts::font_derived::ImportFont,
    fetch_policy: &crate::font_import::catalog::FontFetchPolicy,
    inspection_policy: &crate::font_import::FontImportPolicy,
    checkpoint: &mut dyn crate::font_import::FontImportCheckpoint,
) -> Result<TransactionResult, EditorError> {
    use crate::font_derived::provenance::{expected_import_resource, prepare_derived_font};
    let intent = SceneCommitIntent::DerivedFontImport(request);
    let source = match begin(store, intent)? {
        Ok(source) => source,
        Err(result) => return Ok(result),
    };
    checkpoint.check()?;
    let font = expected_import_resource(request)?;
    let StoredProjectDocument::Editable(project) = &source.document else {
        return Err(EditorError::new(
            "UNSUPPORTED_PROJECT_VERSION",
            "Promote the project before importing fonts",
        ));
    };
    // This internal edit is a pure planner input only. Its family/key is never
    // persisted; receipt and journal remain the exact original import request.
    let operation = ApplySceneProject {
        project_id: request.project_id.clone(),
        expected_revision: request.expected_revision.clone(),
        idempotency_key: request.idempotency_key.clone(),
        label: format!("Import {}", font.family),
        operations: vec![ProjectOperation::FontPut { font: font.clone() }],
    };
    let proposed = plan(
        &EditorSnapshot {
            project: (**project).clone(),
            revision: source.revision.clone(),
        },
        &operation,
    )?;
    let commit = prepare(&source, proposed, intent)?;
    let acquired = prepare_derived_font(
        fetcher,
        inspector,
        request,
        fetch_policy,
        inspection_policy,
        checkpoint,
    )?;
    if acquired.font.resource != font {
        return Err(EditorError::new(
            "FONT_SOURCE_INTEGRITY_ERROR",
            "Inspected font differs from its admitted catalog resource",
        ));
    }
    checkpoint.check()?;
    let _publication = publisher.publish(&acquired.font, &acquired.bytes, checkpoint)?;
    checkpoint.check()?;
    store.commit_scene_atomic(intent, commit)
}

pub fn restore(
    store: &mut impl SceneStore,
    request: &RestoreProject,
) -> Result<TransactionResult, EditorError> {
    let intent = SceneCommitIntent::Restore(request);
    let source = match begin(store, intent)? {
        Ok(source) => source,
        Err(result) => return Ok(result),
    };
    if request
        .label
        .as_ref()
        .is_some_and(|label| label.trim().is_empty() || label.len() > 4096)
    {
        return Err(invalid(
            "Restore review label must be nonempty bounded text",
        ));
    }
    let target = scoped(
        store.revision_scene_source(&request.project_id, &request.target_revision)?,
        &request.project_id,
    )?;
    if target.revision != request.target_revision {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Restore target revision differs",
        ));
    }
    let proposed = converted(&target.document, &request.asset_versions)?;
    store.commit_scene_atomic(intent, prepare(&source, proposed, intent)?)
}

pub(crate) fn prepare(
    source: &StoredProjectRevision,
    project: EditorDocument,
    intent: SceneCommitIntent<'_>,
) -> Result<PreparedSceneCommit, EditorError> {
    if project.id != intent.project_id() || source.document.project_id() != project.id {
        return Err(invalid("Prepared project scope differs"));
    }
    validate_project_scene(&project)?;
    let before = super::entities::collect(&source.document)?;
    let after =
        super::entities::collect(&StoredProjectDocument::Editable(Box::new(project.clone())))?;
    let transaction_id = format!("txn_{}", Uuid::new_v4().simple());
    let material = encode_json(
        &json!({"parent":source.revision,"transaction_id":transaction_id,"document":project}),
        "V4 revision material",
    )?;
    let revision = content_id("rev", material.as_bytes());
    Ok(PreparedSceneCommit {
        journal_json: None,
        snapshot: EditorSnapshot {
            project,
            revision: revision.clone(),
        },
        result: TransactionResult {
            transaction_id,
            base_revision: source.revision.clone(),
            revision,
            created_ids: after
                .keys()
                .filter(|id| !before.contains_key(*id))
                .cloned()
                .collect(),
            changed_ids: after
                .iter()
                .filter(|(id, v)| before.get(*id).is_some_and(|old| old != *v))
                .map(|(id, _)| id.clone())
                .collect(),
            deleted_ids: before
                .keys()
                .filter(|id| !after.contains_key(*id))
                .cloned()
                .collect(),
        },
        event_id: format!("evt_{}", Uuid::new_v4().simple()),
        request_json: intent.request_json()?,
    })
}

/// Captions use the same prepared aggregate edit and ordinary selective history.
pub fn prepare_caption_commit(
    source: &StoredProjectRevision,
    project: EditorDocument,
    intent: SceneCommitIntent<'_>,
) -> Result<PreparedSceneCommit, EditorError> {
    if !matches!(
        intent,
        SceneCommitIntent::CaptionGenerate(_) | SceneCommitIntent::CaptionImport(_)
    ) {
        return Err(invalid(
            "Caption preparation requires its original typed intent",
        ));
    }
    prepare(source, project, intent)
}
