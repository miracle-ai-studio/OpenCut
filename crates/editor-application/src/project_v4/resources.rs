//! Fixed-revision resource selection shares project read authority. No I/O.
use super::*;
use opencut_contracts::project_v4_resources::*;

/// A stream already resolved its immutable resource from a fixed snapshot.
/// Recheck live read scope without reparsing the aggregate for every byte chunk.
pub trait ProjectV4ResourceRead {
    fn require_resource_revision_read(
        &self,
        project_id: &str,
        revision: &str,
    ) -> Result<(), EditorError>;
}
pub fn revalidate_resource_read(
    store: &impl ProjectV4ResourceRead,
    input: &InspectResourceV4,
) -> Result<(), EditorError> {
    store.require_resource_revision_read(&input.project_id, &input.revision)
}

pub fn inspect_resource(
    store: &impl ProjectV4Read,
    request: &InspectResourceV4,
) -> Result<ResourceViewV4, EditorError> {
    for id in [&request.project_id, &request.revision, &request.resource_id] {
        if id.trim().is_empty() || id.len() > 1024 || id.chars().any(char::is_control) {
            return Err(invalid(
                "Resource identifiers must be nonempty bounded text",
            ));
        }
    }
    let snapshot = inspect(
        store,
        &InspectV4Project {
            project_id: request.project_id.clone(),
            revision: Some(request.revision.clone()),
        },
    )?;
    let (version, mime_type) = match request.kind {
        ResourceKindV4::Asset => {
            let asset = snapshot
                .project
                .assets
                .iter()
                .find(|asset| asset.id == request.resource_id)
                .ok_or_else(|| missing("resource"))?;
            (asset.version.clone(), asset.metadata.mime_type.clone())
        }
        ResourceKindV4::Font => {
            let font = snapshot
                .project
                .fonts
                .iter()
                .find(|font| font.id == request.resource_id)
                .ok_or_else(|| missing("resource"))?;
            (font.content.clone(), None)
        }
    };
    Ok(ResourceViewV4 {
        project_id: request.project_id.clone(),
        revision: request.revision.clone(),
        kind: request.kind,
        resource_id: request.resource_id.clone(),
        version,
        mime_type,
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
pub fn content_path(request: &InspectResourceV4) -> String {
    format!(
        "/v1/project-v4-resources/content?project_id={}&revision={}&kind={}&resource_id={}",
        query(&request.project_id),
        query(&request.revision),
        match request.kind {
            ResourceKindV4::Asset => "asset",
            ResourceKindV4::Font => "font",
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
            content_path(&InspectResourceV4 {
                project_id: "../工程&other=1".into(),
                revision: "r+ /?#".into(),
                kind: ResourceKindV4::Font,
                resource_id: "..".into(),
            }),
            "/v1/project-v4-resources/content?project_id=..%2F%E5%B7%A5%E7%A8%8B%26other%3D1&revision=r%2B%20%2F%3F%23&kind=font&resource_id=.."
        );
    }
}
