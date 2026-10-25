use super::*;
use sqlite_project_document::{ProjectStorageDocument, decode_stored_project};

pub(super) fn document_schema(raw: &str, project: &str) -> Result<u32, EditorError> {
    let document = decode_stored_project(raw)?;
    let (id, schema) = match &document {
        ProjectStorageDocument::Legacy(p) => (&p.id, p.schema_version),
        ProjectStorageDocument::HistoricalVisual(p) => (&p.base.id, 3),
        ProjectStorageDocument::Editable(p) => (&p.id, 4),
        ProjectStorageDocument::ReadOnly(p) => (&p.id, 5),
    };
    if id != project {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "History document identity differs from project scope",
        ));
    }
    Ok(schema)
}
fn malformed() -> EditorError {
    EditorError::new(
        "STORAGE_ERROR",
        "History causal journal is malformed or scoped to another request",
    )
}
pub(crate) fn entry(
    project: &str,
    transaction: &str,
    base: &str,
    revision: &str,
    raw: &str,
    before: &str,
    after: &str,
) -> Result<HistoryEntry, EditorError> {
    let before_schema = document_schema(before, project)?;
    let after_schema = document_schema(after, project)?;
    entry_with_schemas(
        project,
        transaction,
        base,
        revision,
        raw,
        before,
        after,
        before_schema,
        after_schema,
    )
}

// The history reader validates adjacent immutable documents once inside its
// SQL snapshot. Single-transition callers above retain full validation here.
pub(super) fn entry_with_schemas(
    project: &str,
    transaction: &str,
    base: &str,
    revision: &str,
    raw: &str,
    before: &str,
    after: &str,
    before_schema: u32,
    after_schema: u32,
) -> Result<HistoryEntry, EditorError> {
    let value: Value = decode_json(raw, "V4 historical journal")?;
    let mut label = value
        .get("label")
        .or_else(|| value.get("request").and_then(|r| r.get("label")))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let kind = match value.get("kind").and_then(Value::as_str) {
        Some("branch_merge") if value.get("request").is_none() => {
            if !matches!(before_schema, 1 | 2) || !matches!(after_schema, 1 | 2) {
                return Err(malformed());
            }
            if !matches!(
                opencut_editor_application::project_archive::ArchivedJournal::from_storage(raw)
                    .map_err(|_| malformed())?,
                opencut_editor_application::project_archive::ArchivedJournal::HistoricalBranchMerge(
                    _
                )
            ) {
                return Err(malformed());
            }
            label = Some(format!(
                "Merge {} into {}",
                value["source_branch"].as_str().ok_or_else(malformed)?,
                value["target_branch"].as_str().ok_or_else(malformed)?
            ));
            HistoryEntryKind::Edit
        }
        Some("branch_apply" | "project_v3.branch_apply" | "project_v3.branch_merge") => {
            let journal =
                opencut_editor_application::project_archive::ArchivedJournal::from_storage(raw)
                    .map_err(|_| malformed())?;
            let visual = value
                .get("kind")
                .and_then(Value::as_str)
                .is_some_and(|kind| kind.starts_with("project_v3."));
            if (visual&&(before_schema!=3||after_schema!=3))||(!visual&&(!matches!(before_schema,1|2)||!matches!(after_schema,1|2)))
                ||!matches!(journal,opencut_editor_application::project_archive::ArchivedJournal::HistoricalBranchApply(_)|opencut_editor_application::project_archive::ArchivedJournal::HistoricalBranchMerge(_)){return Err(malformed());}
            HistoryEntryKind::Edit
        }
        Some("branch_merge") => {
            if before_schema != 4 || after_schema != 4 {
                return Err(malformed());
            }
            let opencut_contracts::branching::BranchJournal::BranchMerge {
                request,
                base_revision,
                changes,
            } = serde_json::from_value(value).map_err(|_| malformed())?;
            opencut_editor_application::branching::validate_compare(
                &opencut_editor_application::branching::comparison(&request),
            )
            .map_err(|_| malformed())?;
            if request.project_id != project
                || request.expected_target_revision != base
                || request.idempotency_key.is_empty()
                || base_revision.is_empty()
                || changes.is_empty()
            {
                return Err(malformed());
            }
            label = Some(format!(
                "Merge {} into {}",
                request.source_branch, request.target_branch
            ));
            HistoryEntryKind::Edit
        }
        Some("project_v4_selective_undo" | "project_v4_selective_redo") => {
            if before_schema != 4 || after_schema != 4 {
                return Err(malformed());
            }
            let journal: CausalJournal = serde_json::from_value(value).map_err(|_| malformed())?;
            match journal {
                CausalJournal::Undo {
                    schema_version,
                    request,
                    ..
                } => {
                    if schema_version != 4
                        || request.project_id != project
                        || request.expected_revision != base
                        || request.target_transaction_id.is_empty()
                    {
                        return Err(malformed());
                    }
                    HistoryEntryKind::Undo {
                        target_transaction_id: request.target_transaction_id,
                    }
                }
                CausalJournal::Redo {
                    schema_version,
                    request,
                    target_transaction_id,
                    ..
                } => {
                    if schema_version != 4
                        || request.project_id != project
                        || request.expected_revision != base
                        || target_transaction_id.is_empty()
                        || request.undo_transaction_id.is_empty()
                    {
                        return Err(malformed());
                    }
                    HistoryEntryKind::Redo {
                        target_transaction_id,
                        undo_transaction_id: request.undo_transaction_id,
                    }
                }
            }
        }
        Some("selective_undo" | "project_v3.selective_undo") => HistoryEntryKind::Undo {
            target_transaction_id: value
                .get("target_transaction_id")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .ok_or_else(malformed)?
                .into(),
        },
        Some("selective_redo" | "project_v3.selective_redo") => HistoryEntryKind::Redo {
            target_transaction_id: value
                .get("target_transaction_id")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .ok_or_else(malformed)?
                .into(),
            undo_transaction_id: value
                .get("undo_transaction_id")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .ok_or_else(malformed)?
                .into(),
        },
        Some("project_v3.create" | "project_v3.promote") => HistoryEntryKind::Promotion,
        Some(kind)
            if kind.starts_with("selective_")
                || kind.starts_with("project_v4_selective_")
                || kind.starts_with("project_v3.selective_") =>
        {
            return Err(malformed());
        }
        _ if before_schema != after_schema => HistoryEntryKind::Promotion,
        _ if after_schema == 4 => {
            // Published initial V4 edits used a bare typed request. Preserve
            // those bytes and infer their complete effect from both revisions.
            if let Ok(request) = serde_json::from_value::<ApplySceneProject>(value.clone()) {
                if request.project_id != project || request.expected_revision != base {
                    return Err(malformed());
                }
            } else if let Ok(request) = serde_json::from_value::<
                opencut_contracts::project_public::ApplyProject,
            >(value.clone())
            {
                if request.project_id != project || request.expected_revision != base {
                    return Err(malformed());
                }
            } else if let Ok(request) =
                serde_json::from_value::<opencut_contracts::font_import::ImportFont>(value.clone())
            {
                if request.project_id != project || request.expected_revision != base {
                    return Err(malformed());
                }
                // Both original import DTOs have the same frozen shape. Their
                // pinned catalog IDs distinguish the source authority, while
                // the stored journal remains the complete original request.
                let font = if opencut_editor_application::font_derived::provenance::source(
                    &request.catalog_id,
                )
                .is_ok()
                {
                    let derived_request: opencut_contracts::font_derived::ImportFont =
                        serde_json::from_value(value.clone()).map_err(|_| malformed())?;
                    opencut_editor_application::font_derived::provenance::expected_import_resource(
                        &derived_request,
                    )
                } else {
                    opencut_editor_application::font_import::expected_import_resource(&request)
                }
                .map_err(|_| malformed())?;
                let ProjectStorageDocument::Editable(before_document) =
                    decode_stored_project(before)?
                else {
                    return Err(malformed());
                };
                let ProjectStorageDocument::Editable(after_document) =
                    decode_stored_project(after)?
                else {
                    return Err(malformed());
                };
                let expected = opencut_editor_application::scene::plan(
                    &opencut_contracts::scene::EditorSnapshot {
                        project: before_document,
                        revision: base.into(),
                    },
                    &ApplySceneProject {
                        project_id: request.project_id,
                        expected_revision: request.expected_revision,
                        idempotency_key: request.idempotency_key,
                        label: format!("Import {}", font.family),
                        operations: vec![ProjectOperation::FontPut { font: font.clone() }],
                    },
                )
                .map_err(|_| malformed())?;
                if expected != after_document {
                    return Err(malformed());
                }
                label = Some(format!("Import {}", font.family));
            } else if let Ok(request) = serde_json::from_value::<
                opencut_api_contracts::subtitles::GenerateCaptions,
            >(value.clone())
            {
                if request.project_id != project || request.expected_revision != base {
                    return Err(malformed());
                }
                label = Some("Generate captions".into());
            } else if let Ok(request) = serde_json::from_value::<
                opencut_api_contracts::subtitles::SubtitleImport,
            >(value.clone())
            {
                if request.project_id != project || request.expected_revision != base {
                    return Err(malformed());
                }
                label = Some("Import subtitles".into());
            } else if value
                .get("catalog_id")
                .and_then(Value::as_str)
                .is_some_and(|id| id.starts_with("simple-icons:"))
            {
                let opencut_editor_application::project_archive::ArchivedJournal::BrandImport(
                    journal,
                ) = opencut_editor_application::project_archive::ArchivedJournal::from_storage(raw)
                    .map_err(|_| malformed())?
                else {
                    return Err(malformed());
                };
                let request = journal.request();
                if request.project_id != project || request.expected_revision != base {
                    return Err(malformed());
                }
                opencut_editor_application::brand_resources::validate_request(&request)
                    .map_err(|_| malformed())?;
                let ProjectStorageDocument::Editable(before_document) =
                    decode_stored_project(before)?
                else {
                    return Err(malformed());
                };
                let ProjectStorageDocument::Editable(after_document) =
                    decode_stored_project(after)?
                else {
                    return Err(malformed());
                };
                let asset = after_document
                    .assets
                    .iter()
                    .find(|asset| asset.id == request.asset_id)
                    .ok_or_else(malformed)?;
                if before_document
                    .assets
                    .iter()
                    .any(|asset| asset.id == request.asset_id)
                    || opencut_editor_application::brand_resources::bind_resource(
                        &request,
                        asset.clone(),
                    )
                    .map_err(|_| malformed())?
                        != *asset
                {
                    return Err(malformed());
                }
                let expected = opencut_editor_application::scene::plan(
                    &opencut_contracts::scene::EditorSnapshot {
                        project: before_document,
                        revision: base.into(),
                    },
                    &ApplySceneProject {
                        project_id: request.project_id.clone(),
                        expected_revision: request.expected_revision.clone(),
                        idempotency_key: request.idempotency_key.clone(),
                        label: format!("Acquire brand {}", asset.name),
                        operations: vec![ProjectOperation::AssetPut {
                            asset: asset.clone(),
                        }],
                    },
                )
                .map_err(|_| malformed())?;
                if expected != after_document {
                    return Err(malformed());
                }
                label = Some("Import brand resource".into());
            } else if let Ok(request) = serde_json::from_value::<
                opencut_contracts::catalog_resources::ImportCatalogResource,
            >(value.clone())
            {
                if request.project_id != project || request.expected_revision != base {
                    return Err(malformed());
                }
                label = Some("Import catalog resource".into());
            } else if let Ok(request) = serde_json::from_value::<
                opencut_contracts::catalog_resources::GenerateCatalogSound,
            >(value.clone())
            {
                if request.project_id != project || request.expected_revision != base {
                    return Err(malformed());
                }
                label = Some("Generate catalog sound".into());
            } else if let Ok(request) = serde_json::from_value::<RestoreProject>(value) {
                if request.project_id != project || request.expected_revision != base {
                    return Err(malformed());
                }
            } else {
                return Err(malformed());
            }
            HistoryEntryKind::Edit
        }
        _ => HistoryEntryKind::Edit,
    };
    Ok(HistoryEntry {
        transaction_id: transaction.into(),
        base_revision: base.into(),
        revision: revision.into(),
        label,
        kind,
    })
}
