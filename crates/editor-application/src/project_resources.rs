//! Fixed-revision resource selection shares project read authority. No I/O.
use crate::scene::{SceneRead, StoredProjectDocument};
use opencut_contracts::EditorError;
use opencut_contracts::project_resources::*;

/// A stream already resolved its immutable resource from a fixed snapshot.
/// Recheck live read scope without reparsing the aggregate for every byte chunk.
pub trait ProjectResourceRead {
    fn require_resource_revision_read(
        &self,
        project_id: &str,
        revision: &str,
    ) -> Result<(), EditorError>;
}
/// Resource descriptors carry a media type,never a historical source locator.
pub(crate) fn media_mime(value: Option<&str>) -> Option<String> {
    let value = value?;
    let (kind, subtype) = value.split_once('/')?;
    (matches!(
        kind,
        "application"
            | "audio"
            | "font"
            | "image"
            | "message"
            | "model"
            | "multipart"
            | "text"
            | "video"
    ) && !subtype.is_empty()
        && value.len() <= 128
        && subtype
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"+._-".contains(&b)))
    .then(|| value.to_owned())
}
pub fn revalidate_resource_read(
    store: &impl ProjectResourceRead,
    input: &InspectResource,
) -> Result<(), EditorError> {
    store.require_resource_revision_read(&input.project_id, &input.revision)
}

pub fn inspect_resource(
    store: &impl SceneRead,
    request: &InspectResource,
) -> Result<ResourceView, EditorError> {
    for id in [&request.project_id, &request.revision, &request.resource_id] {
        if id.trim().is_empty() || id.len() > 1024 || id.chars().any(char::is_control) {
            return Err(EditorError::new(
                "INVALID_ARGUMENT",
                "Resource identifiers must be nonempty bounded text",
            ));
        }
    }
    let source = store.revision_scene_source(&request.project_id, &request.revision)?;
    if source.document.project_id() != request.project_id || source.revision != request.revision {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Resource scope differs from its immutable revision",
        ));
    }
    let (version, mime_type) = match &source.document {
        StoredProjectDocument::Editable(project) => match request.kind {
            ResourceKind::Asset => {
                let asset = project
                    .assets
                    .iter()
                    .find(|a| a.id == request.resource_id)
                    .ok_or_else(|| {
                        EditorError::new("NOT_FOUND", "Project resource does not exist")
                    })?;
                (asset.version.clone(), asset.metadata.mime_type.clone())
            }
            ResourceKind::Font => {
                let font = project
                    .fonts
                    .iter()
                    .find(|f| f.id == request.resource_id)
                    .ok_or_else(|| {
                        EditorError::new("NOT_FOUND", "Project resource does not exist")
                    })?;
                (font.content.clone(), None)
            }
        },
        historical => {
            if request.kind != ResourceKind::Asset {
                return Err(EditorError::new(
                    "NOT_FOUND",
                    "Project resource does not exist",
                ));
            }
            let assets = match historical {
                StoredProjectDocument::Legacy(project) => &project.assets,
                StoredProjectDocument::HistoricalVisual(project) => &project.base.assets,
                _ => unreachable!(),
            };
            let asset = assets
                .iter()
                .find(|a| a.id == request.resource_id)
                .ok_or_else(|| EditorError::new("NOT_FOUND", "Project resource does not exist"))?;
            let version = crate::media_facts::legacy_asset_version(asset).ok_or_else(|| {
                EditorError::new(
                    "ASSET_LOCK_REQUIRED",
                    "Historical resource has no immutable identity",
                )
            })?;
            (
                version,
                asset
                    .metadata
                    .get("mime_type")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned),
            )
        }
    };
    opencut_editor_core::validate_asset_version(&version)?;
    Ok(ResourceView {
        project_id: request.project_id.clone(),
        revision: request.revision.clone(),
        kind: request.kind,
        resource_id: request.resource_id.clone(),
        version,
        mime_type: media_mime(mime_type.as_deref()),
        content_path: content_path(request),
    })
}

fn query(value: &str) -> String {
    let mut result = String::new();
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            result.push(char::from(byte));
        } else {
            result.push('%');
            result.push(char::from(HEX[usize::from(byte >> 4)]));
            result.push(char::from(HEX[usize::from(byte & 15)]));
        }
    }
    result
}

/// Query identifiers cannot become path traversal segments after URL parsing.
pub fn content_path(request: &InspectResource) -> String {
    format!(
        "/api/project-resources/content?project_id={}&revision={}&kind={}&resource_id={}",
        query(&request.project_id),
        query(&request.revision),
        match request.kind {
            ResourceKind::Asset => "asset",
            ResourceKind::Font => "font",
        },
        query(&request.resource_id)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn query_ids_preserve_unicode_and_never_change_the_resource_path() {
        assert_eq!(
            content_path(&InspectResource {
                project_id: "../工程&other=1".into(),
                revision: "r+ /?#".into(),
                kind: ResourceKind::Font,
                resource_id: "..".into(),
            }),
            "/api/project-resources/content?project_id=..%2F%E5%B7%A5%E7%A8%8B%26other%3D1&revision=r%2B%20%2F%3F%23&kind=font&resource_id=.."
        );
    }
}
