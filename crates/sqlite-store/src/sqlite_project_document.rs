//! Strict stored-document readers. Original formats never discard richer content.
use super::*;
use opencut_contracts::{
    ProjectDocument, historical_visual::HistoricalVisualDocument, scene::EditorDocument,
};

pub(super) enum ProjectStorageDocument {
    Legacy(ProjectDocument),
    HistoricalVisual(HistoricalVisualDocument),
    Editable(EditorDocument),
    ReadOnly(opencut_contracts::project_readonly::ReadOnlyProjectDocument),
}

pub(super) fn decode_stored_project(raw: &str) -> Result<ProjectStorageDocument, EditorError> {
    let value: Value = decode_json(raw, "stored project document")?;
    match value.get("schema_version").and_then(Value::as_u64) {
        Some(5) => {
            let project: opencut_contracts::project_readonly::ReadOnlyProjectDocument =
                serde_json::from_value(value).map_err(|_| {
                    EditorError::new("STORAGE_ERROR", "Invalid preserved project document")
                })?;
            opencut_editor_application::project_readonly::validate_document(&project).map_err(
                |_| EditorError::new("STORAGE_ERROR", "Invalid preserved project graph"),
            )?;
            Ok(ProjectStorageDocument::ReadOnly(project))
        }
        Some(4) => {
            let project: EditorDocument = serde_json::from_value(value).map_err(|e| {
                EditorError::new(
                    "STORAGE_ERROR",
                    format!("Invalid schema 4 project document: {e}"),
                )
            })?;
            opencut_editor_core::scene::validate_project_scene(&project).map_err(|e| {
                EditorError::new(
                    "STORAGE_ERROR",
                    format!("Invalid schema 4 project document: {}", e.message),
                )
            })?;
            Ok(ProjectStorageDocument::Editable(project))
        }
        Some(3) => {
            let project: HistoricalVisualDocument = serde_json::from_value(value).map_err(|e| {
                EditorError::new(
                    "STORAGE_ERROR",
                    format!("Invalid schema 3 project document: {e}"),
                )
            })?;
            opencut_editor_core::historical_visual::validate_project_historical_visual(&project)
                .map_err(|e| {
                    EditorError::new(
                        "STORAGE_ERROR",
                        format!("Invalid schema 3 project document: {}", e.message),
                    )
                })?;
            Ok(ProjectStorageDocument::HistoricalVisual(project))
        }
        Some(1 | 2) => Ok(ProjectStorageDocument::Legacy(
            serde_json::from_value(value).map_err(|e| {
                EditorError::new(
                    "STORAGE_ERROR",
                    format!("Invalid legacy project document: {e}"),
                )
            })?,
        )),
        _ => Err(EditorError::new(
            "UNSUPPORTED_PROJECT_VERSION",
            "Unknown project document schema version",
        )),
    }
}

pub(super) fn decode_legacy_project(raw: &str) -> Result<ProjectDocument, EditorError> {
    match decode_stored_project(raw)? {
        ProjectStorageDocument::Legacy(project) => Ok(project),
        ProjectStorageDocument::ReadOnly(_) => {
            Err(opencut_editor_application::project_readonly::readonly_error())
        }
        ProjectStorageDocument::HistoricalVisual(_) | ProjectStorageDocument::Editable(_) => {
            Err(EditorError::new(
                "UNSUPPORTED_PROJECT_VERSION",
                "This project requires its versioned project tools",
            ))
        }
    }
}
/// Validate scope without projecting a newer aggregate into a legacy document.
pub(super) fn project_head_revision(
    connection: &Connection,
    project: &str,
) -> Result<String, EditorError> {
    sqlite_project_library::require_live(connection, project)?;
    let row: Option<(String, String)> = connection
        .query_row(
            "SELECT r.document_json,p.head_revision
        FROM projects p JOIN revisions r ON r.revision=p.head_revision AND r.project_id=p.project_id
        WHERE p.project_id=?1",
            [project],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(storage_error)?;
    let (raw, revision) =
        row.ok_or_else(|| EditorError::new("NOT_FOUND", "Project does not exist"))?;
    let actual = match decode_stored_project(&raw)? {
        ProjectStorageDocument::Legacy(document) => document.id,
        ProjectStorageDocument::HistoricalVisual(document) => document.base.id,
        ProjectStorageDocument::Editable(document) => document.id,
        ProjectStorageDocument::ReadOnly(document) => document.id,
    };
    if actual != project {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Project identity differs from scope",
        ));
    }
    Ok(revision)
}
pub(super) fn decode_historical_visual_project(
    raw: &str,
) -> Result<HistoricalVisualDocument, EditorError> {
    match decode_stored_project(raw)? {
        ProjectStorageDocument::HistoricalVisual(project) => Ok(project),
        ProjectStorageDocument::ReadOnly(_) => {
            Err(opencut_editor_application::project_readonly::readonly_error())
        }
        ProjectStorageDocument::Legacy(_) | ProjectStorageDocument::Editable(_) => {
            Err(EditorError::new(
                "UNSUPPORTED_PROJECT_VERSION",
                "Project revision requires a different versioned project tool",
            ))
        }
    }
}

/// Collection-only expanded view. Original opaque strings stay unchanged in the
/// revision; parsed values expose escaped artifact/CAS references conservatively.
pub(super) fn reference_document(raw: &str) -> Result<(Value, usize), EditorError> {
    let mut value: Value = decode_json(raw, "reference document")?;
    let mut extra = 0usize;
    if value.get("schema_version").and_then(Value::as_u64) == Some(5) {
        let ProjectStorageDocument::ReadOnly(document) = decode_stored_project(raw)? else {
            unreachable!()
        };
        let mut payloads = Vec::new();
        for effect in &document.opaque_effects {
            extra = extra.saturating_add(effect.payload_json.len());
            // Decode every JSON string token, including overwritten duplicate
            // keys/values. Parsing into Value alone would discard a dependency
            // stored in an earlier duplicate key. Validation above already
            // established valid, bounded JSON; this is a conservative scan.
            let bytes = effect.payload_json.as_bytes();
            let mut cursor = 0;
            while cursor < bytes.len() {
                if bytes[cursor] != b'"' {
                    cursor += 1;
                    continue;
                }
                let start = cursor;
                cursor += 1;
                while cursor < bytes.len() {
                    match bytes[cursor] {
                        b'\\' => cursor += 2,
                        b'"' => {
                            cursor += 1;
                            break;
                        }
                        _ => cursor += 1,
                    }
                }
                let token: String = serde_json::from_str(&effect.payload_json[start..cursor])
                    .map_err(|_| {
                        EditorError::new("STORAGE_ERROR", "Invalid preserved reference token")
                    })?;
                payloads.push(Value::String(token));
            }
        }
        value
            .as_object_mut()
            .ok_or_else(|| EditorError::new("STORAGE_ERROR", "Invalid preserved reference root"))?
            .insert(
                "__preserved_reference_values".into(),
                Value::Array(payloads),
            );
    }
    Ok((value, extra))
}
