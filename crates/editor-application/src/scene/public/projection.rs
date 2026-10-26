use super::*;

/// Conservative provenance URL policy: preserve plain public HTTP(S) locators,
/// hide credentials, query/fragment-bearing or ambiguous locators wholesale.
/// Never rewrite a signed URL into a different source or follow it here.
fn source_url(value: &str) -> Option<String> {
    let rest = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"))?;
    if value
        .bytes()
        .any(|b| b.is_ascii_control() || b.is_ascii_whitespace())
        || value.contains(['\\', '@', '?', '#', '%'])
    {
        return None;
    }
    let authority = rest.split('/').next()?;
    if authority.is_empty()
        || !authority
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-:".contains(&b))
    {
        return None;
    }
    // ASCII DNS/IPv4 with an optional numeric port. This intentionally excludes
    // ambiguous URL forms rather than inferring a safe interpretation.
    let mut host_port = authority.split(':');
    let host = host_port.next()?;
    if host.is_empty() || host.starts_with('.') || host.ends_with('.') {
        return None;
    }
    if let Some(port) = host_port.next()
        && (port.is_empty() || port.parse::<u16>().is_err())
    {
        return None;
    }
    if host_port.next().is_some() {
        return None;
    }
    Some(value.into())
}
fn origin(value: &ResourceOrigin) -> PublicResourceOrigin {
    match value {
        ResourceOrigin::LegacyLocator { .. } => PublicResourceOrigin::Legacy {},
        ResourceOrigin::LocalImport { original_name } => {
            let name = original_name.rsplit(['/', '\\']).next().filter(|name| {
                !name.is_empty()
                    && *name != "."
                    && *name != ".."
                    && !name.chars().any(char::is_control)
            });
            PublicResourceOrigin::LocalImport {
                original_name: name.map(str::to_owned),
            }
        }
        ResourceOrigin::Online {
            provider,
            source_url: url,
        } => PublicResourceOrigin::Online {
            provider: provider.clone(),
            source_url: source_url(url),
        },
        ResourceOrigin::Generated {
            provider,
            generation_id,
        } => PublicResourceOrigin::Generated {
            provider: provider.clone(),
            generation_id: (!generation_id.is_empty()
                && generation_id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b)))
            .then(|| generation_id.clone()),
        },
    }
}
fn license(value: &Option<ResourceLicense>) -> Option<ResourceLicense> {
    value.as_ref().map(|value| ResourceLicense {
        identifier: value.identifier.clone(),
        source_url: value.source_url.as_deref().and_then(source_url),
        notice: value.notice.clone(),
        attribution: value.attribution.clone(),
    })
}
pub fn project(snapshot: &EditorSnapshot) -> PublicProjectSnapshot {
    let value = &snapshot.project;
    PublicProjectSnapshot {
        revision: snapshot.revision.clone(),
        project: PublicProjectDocument {
            id: value.id.clone(),
            name: value.name.clone(),
            default_sequence_id: value.default_sequence_id.clone(),
            settings: value.settings.clone(),
            cover: value.cover.clone(),
            sequences: value.sequences.clone(),
            assets: value
                .assets
                .iter()
                .map(|asset| PublicAsset {
                    id: asset.id.clone(),
                    name: asset.name.clone(),
                    kind: asset.kind.clone(),
                    version: asset.version.clone(),
                    duration: asset.duration.clone(),
                    origin: origin(&asset.origin),
                    metadata: asset.metadata.clone(),
                    license: license(&asset.license),
                })
                .collect(),
            fonts: value
                .fonts
                .iter()
                .map(|font| PublicFontResource {
                    id: font.id.clone(),
                    family: font.family.clone(),
                    weight: font.weight,
                    style: font.style,
                    face_index: font.face_index,
                    content: font.content.clone(),
                    origin: origin(&font.origin),
                    license: license(&font.license),
                })
                .collect(),
        },
    }
}
