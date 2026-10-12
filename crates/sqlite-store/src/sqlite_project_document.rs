//! Shared stored-document decoding. Frozen versioned readers never project newer content.
use super::*;
use opencut_contracts::{
    ProjectDocument, project_v3::ProjectDocumentV3, project_v4::ProjectDocumentV4,
};

pub(super) enum VersionedProjectDocument {
    Legacy(ProjectDocument),
    V3(ProjectDocumentV3),
    V4(ProjectDocumentV4),
}

pub(super) fn decode_versioned_project(raw: &str) -> Result<VersionedProjectDocument, EditorError> {
    let value: Value = decode_json(raw, "stored project document")?;
    match value.get("schema_version").and_then(Value::as_u64) {
        Some(4) => {
            let project: ProjectDocumentV4 = serde_json::from_value(value).map_err(|e| {
                EditorError::new(
                    "STORAGE_ERROR",
                    format!("Invalid schema 4 project document: {e}"),
                )
            })?;
            opencut_editor_core::project_v4::validate_project_v4(&project).map_err(|e| {
                EditorError::new(
                    "STORAGE_ERROR",
                    format!("Invalid schema 4 project document: {}", e.message),
                )
            })?;
            Ok(VersionedProjectDocument::V4(project))
        }
        Some(3) => {
            let project: ProjectDocumentV3 = serde_json::from_value(value).map_err(|e| {
                EditorError::new(
                    "STORAGE_ERROR",
                    format!("Invalid schema 3 project document: {e}"),
                )
            })?;
            opencut_editor_core::project_v3::validate_project_v3(&project).map_err(|e| {
                EditorError::new(
                    "STORAGE_ERROR",
                    format!("Invalid schema 3 project document: {}", e.message),
                )
            })?;
            Ok(VersionedProjectDocument::V3(project))
        }
        Some(1 | 2) => Ok(VersionedProjectDocument::Legacy(
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
    match decode_versioned_project(raw)? {
        VersionedProjectDocument::Legacy(project) => Ok(project),
        VersionedProjectDocument::V3(_) | VersionedProjectDocument::V4(_) => Err(EditorError::new(
            "UNSUPPORTED_PROJECT_VERSION",
            "This project requires its versioned project tools",
        )),
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
    let actual = match decode_versioned_project(&raw)? {
        VersionedProjectDocument::Legacy(document) => document.id,
        VersionedProjectDocument::V3(document) => document.base.id,
        VersionedProjectDocument::V4(document) => document.id,
    };
    if actual != project {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Project identity differs from scope",
        ));
    }
    Ok(revision)
}
pub(super) fn decode_v3_project(raw: &str) -> Result<ProjectDocumentV3, EditorError> {
    match decode_versioned_project(raw)? {
        VersionedProjectDocument::V3(project) => Ok(project),
        VersionedProjectDocument::Legacy(_) | VersionedProjectDocument::V4(_) => {
            Err(EditorError::new(
                "UNSUPPORTED_PROJECT_VERSION",
                "Project revision requires a different versioned project tool",
            ))
        }
    }
}
