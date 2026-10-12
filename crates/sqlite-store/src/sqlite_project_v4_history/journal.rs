use super::*;
use sqlite_project_document::{VersionedProjectDocument, decode_versioned_project};

pub(super) fn document_schema(raw: &str, project: &str) -> Result<u32, EditorError> {
    let document = decode_versioned_project(raw)?;
    let (id, schema) = match &document {
        VersionedProjectDocument::Legacy(p) => (&p.id, p.schema_version),
        VersionedProjectDocument::V3(p) => (&p.base.id, 3),
        VersionedProjectDocument::V4(p) => (&p.id, 4),
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
pub(super) fn entry(
    project: &str,
    transaction: &str,
    base: &str,
    revision: &str,
    raw: &str,
    before: &str,
    after: &str,
) -> Result<HistoryEntryV4, EditorError> {
    let before_schema = document_schema(before, project)?;
    let after_schema = document_schema(after, project)?;
    let value: Value = decode_json(raw, "V4 historical journal")?;
    let label = value
        .get("label")
        .or_else(|| value.get("request").and_then(|r| r.get("label")))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let kind = match value.get("kind").and_then(Value::as_str) {
        Some("project_v4_selective_undo" | "project_v4_selective_redo") => {
            if before_schema != 4 || after_schema != 4 {
                return Err(malformed());
            }
            let journal: CausalJournalV4 =
                serde_json::from_value(value).map_err(|_| malformed())?;
            match journal {
                CausalJournalV4::Undo {
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
                    HistoryEntryKindV4::Undo {
                        target_transaction_id: request.target_transaction_id,
                    }
                }
                CausalJournalV4::Redo {
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
                    HistoryEntryKindV4::Redo {
                        target_transaction_id,
                        undo_transaction_id: request.undo_transaction_id,
                    }
                }
            }
        }
        Some("selective_undo" | "project_v3.selective_undo") => HistoryEntryKindV4::Undo {
            target_transaction_id: value
                .get("target_transaction_id")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .ok_or_else(malformed)?
                .into(),
        },
        Some("selective_redo" | "project_v3.selective_redo") => HistoryEntryKindV4::Redo {
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
        Some("project_v3.create" | "project_v3.promote") => HistoryEntryKindV4::Promotion,
        Some(kind)
            if kind.starts_with("selective_")
                || kind.starts_with("project_v4_selective_")
                || kind.starts_with("project_v3.selective_") =>
        {
            return Err(malformed());
        }
        _ if before_schema != after_schema => HistoryEntryKindV4::Promotion,
        _ if after_schema == 4 => {
            // Published initial V4 edits used a bare typed request. Preserve
            // those bytes and infer their complete effect from both revisions.
            if let Ok(request) = serde_json::from_value::<ApplyV4Project>(value.clone()) {
                if request.project_id != project || request.expected_revision != base {
                    return Err(malformed());
                }
            } else if let Ok(request) = serde_json::from_value::<
                opencut_contracts::project_v4_public::ApplyPublicV4Project,
            >(value.clone())
            {
                if request.project_id != project || request.expected_revision != base {
                    return Err(malformed());
                }
            } else if let Ok(request) = serde_json::from_value::<RestoreV4Revision>(value) {
                if request.project_id != project || request.expected_revision != base {
                    return Err(malformed());
                }
            } else {
                return Err(malformed());
            }
            HistoryEntryKindV4::Edit
        }
        _ => HistoryEntryKindV4::Edit,
    };
    Ok(HistoryEntryV4 {
        transaction_id: transaction.into(),
        base_revision: base.into(),
        revision: revision.into(),
        label,
        kind,
    })
}
