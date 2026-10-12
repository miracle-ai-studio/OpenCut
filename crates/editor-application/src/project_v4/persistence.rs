use super::*;
use serde_json::json;

pub fn list(
    store: &impl ProjectV4LibraryRead,
    request: &ListV4Projects,
) -> Result<V4ProjectPage, EditorError> {
    if !(1..=100).contains(&request.limit) {
        return Err(invalid("Project page limit must be 1..100"));
    }
    store.list_v4_projects(request)
}
pub fn copy(
    store: &mut impl ProjectV4Store,
    request: &CopyV4Project,
) -> Result<ProjectSnapshotV4, EditorError> {
    require_idempotency_key(&request.idempotency_key)?;
    store.authorize_v4(V4Intent::Copy(request))?;
    if let Some(result) = store.replay_v4_copy(request)? {
        return Ok(result);
    }
    // Copy is a V4 aggregate operation, not an implicit legacy migration.
    let source = inspect(
        store,
        &InspectV4Project {
            project_id: request.source_project_id.clone(),
            revision: Some(request.source_revision.clone()),
        },
    )?;
    let mut project = source.project;
    project.id = request.project_id.clone();
    project.name = request.name.clone();
    validate_project_v4(&project)?;
    let revision = content_id(
        "rev",
        encode_json(&project, "V4 copied project")?.as_bytes(),
    );
    store.copy_v4_atomic(
        request,
        PreparedV4Create {
            snapshot: ProjectSnapshotV4 { project, revision },
            event_id: format!("evt_{}", Uuid::new_v4().simple()),
            request_json: V4Intent::Copy(request).request_json()?,
        },
    )
}

pub fn create(
    store: &mut impl ProjectV4Store,
    request: &CreateV4Project,
) -> Result<ProjectSnapshotV4, EditorError> {
    require_idempotency_key(&request.idempotency_key)?;
    store.authorize_v4(V4Intent::Create(request))?;
    if let Some(result) = store.replay_v4_create(request)? {
        return Ok(result);
    }
    match store.head_v4_source(&request.project.id) {
        Ok(_) => {
            if let Some(result) = store.replay_v4_create(request)? {
                return Ok(result);
            }
            return Err(EditorError::new("ALREADY_EXISTS", "Project already exists"));
        }
        Err(error) if error.code == "NOT_FOUND" => {}
        Err(error) => return Err(error),
    }
    let project = new_document(&request.project)?;
    let revision = content_id("rev", encode_json(&project, "V4 project")?.as_bytes());
    store.create_v4_atomic(
        request,
        PreparedV4Create {
            snapshot: ProjectSnapshotV4 { project, revision },
            event_id: format!("evt_{}", Uuid::new_v4().simple()),
            request_json: V4Intent::Create(request).request_json()?,
        },
    )
}

fn scoped(
    source: StoredProjectRevisionV4,
    project_id: &str,
) -> Result<StoredProjectRevisionV4, EditorError> {
    if source.document.project_id() != project_id || source.revision.trim().is_empty() {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Stored revision differs from requested project scope",
        ));
    }
    Ok(source)
}
pub fn inspect(
    store: &impl ProjectV4Read,
    request: &InspectV4Project,
) -> Result<ProjectSnapshotV4, EditorError> {
    let source = match request.revision.as_deref() {
        Some(revision) => {
            let source = store.revision_v4_source(&request.project_id, revision)?;
            if source.revision != revision {
                return Err(EditorError::new(
                    "STORAGE_ERROR",
                    "Fixed revision read changed revision",
                ));
            }
            source
        }
        None => store.head_v4_source(&request.project_id)?,
    };
    let source = scoped(source, &request.project_id)?;
    let StoredProjectDocumentV4::V4(project) = source.document else {
        return Err(EditorError::new(
            "UNSUPPORTED_PROJECT_VERSION",
            "V4 inspection requires a promoted revision",
        ));
    };
    validate_project_v4(&project)?;
    Ok(ProjectSnapshotV4 {
        project: *project,
        revision: source.revision,
    })
}

pub(super) fn checked_head(
    store: &impl ProjectV4Read,
    intent: V4Intent<'_>,
) -> Result<StoredProjectRevisionV4, EditorError> {
    let expected = intent
        .expected_revision()
        .ok_or_else(|| invalid("Edit requires expected revision"))?;
    if expected.trim().is_empty() {
        return Err(invalid("Expected revision must be nonempty"));
    }
    let source = scoped(
        store.head_v4_source(intent.project_id())?,
        intent.project_id(),
    )?;
    if source.revision != expected {
        return Err(revision_conflict(expected, &source.revision));
    }
    Ok(source)
}
pub(super) fn begin(
    store: &impl ProjectV4Read,
    intent: V4Intent<'_>,
) -> Result<Result<StoredProjectRevisionV4, TransactionResult>, EditorError> {
    require_idempotency_key(intent.key())?;
    store.authorize_v4(intent)?;
    if let Some(result) = store.replay_v4_edit(intent)? {
        return Ok(Err(result));
    }
    match checked_head(store, intent) {
        Ok(source) => Ok(Ok(source)),
        Err(error) if error.code == "REVISION_CONFLICT" => {
            if let Some(result) = store.replay_v4_edit(intent)? {
                return Ok(Err(result));
            }
            Err(error)
        }
        Err(error) => Err(error),
    }
}
pub fn promote(
    store: &mut impl ProjectV4Store,
    request: &PromoteV4Project,
) -> Result<TransactionResult, EditorError> {
    let intent = V4Intent::Promote(request);
    let source = match begin(store, intent)? {
        Ok(source) => source,
        Err(result) => return Ok(result),
    };
    if matches!(source.document, StoredProjectDocumentV4::V4(_)) {
        return Err(invalid("Project is already V4"));
    }
    let proposed = converted(&source.document, &request.asset_versions)?;
    store.commit_v4_atomic(intent, prepare(&source, proposed, intent)?)
}
pub fn apply(
    store: &mut impl ProjectV4Store,
    request: &ApplyV4Project,
) -> Result<TransactionResult, EditorError> {
    let intent = V4Intent::Apply(request);
    let source = match begin(store, intent)? {
        Ok(source) => source,
        Err(result) => return Ok(result),
    };
    let StoredProjectDocumentV4::V4(project) = &source.document else {
        return Err(EditorError::new(
            "UNSUPPORTED_PROJECT_VERSION",
            "Promote the project before applying V4 edits",
        ));
    };
    let snapshot = ProjectSnapshotV4 {
        project: (**project).clone(),
        revision: source.revision.clone(),
    };
    let proposed = plan(&snapshot, request)?;
    store.commit_v4_atomic(intent, prepare(&source, proposed, intent)?)
}
pub fn restore(
    store: &mut impl ProjectV4Store,
    request: &RestoreV4Revision,
) -> Result<TransactionResult, EditorError> {
    let intent = V4Intent::Restore(request);
    let source = match begin(store, intent)? {
        Ok(source) => source,
        Err(result) => return Ok(result),
    };
    let target = scoped(
        store.revision_v4_source(&request.project_id, &request.target_revision)?,
        &request.project_id,
    )?;
    if target.revision != request.target_revision {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Restore target revision differs",
        ));
    }
    let proposed = converted(&target.document, &request.asset_versions)?;
    store.commit_v4_atomic(intent, prepare(&source, proposed, intent)?)
}

pub(super) fn prepare(
    source: &StoredProjectRevisionV4,
    project: ProjectDocumentV4,
    intent: V4Intent<'_>,
) -> Result<PreparedV4Commit, EditorError> {
    if project.id != intent.project_id() || source.document.project_id() != project.id {
        return Err(invalid("Prepared project scope differs"));
    }
    validate_project_v4(&project)?;
    let before = super::entities::collect(&source.document)?;
    let after = super::entities::collect(&StoredProjectDocumentV4::V4(Box::new(project.clone())))?;
    let transaction_id = format!("txn_{}", Uuid::new_v4().simple());
    let material = encode_json(
        &json!({"parent":source.revision,"transaction_id":transaction_id,"document":project}),
        "V4 revision material",
    )?;
    let revision = content_id("rev", material.as_bytes());
    Ok(PreparedV4Commit {
        journal_json: None,
        snapshot: ProjectSnapshotV4 {
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
