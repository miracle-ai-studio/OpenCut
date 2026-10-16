//! Pure single-revision V4 package validation. Native files and prepared import
//! authority stay outside public DTOs; every declared resource is accounted for.
use opencut_api_contracts::project_package_v4::{PackageManifestV4, PackageResourceV4};
use opencut_contracts::{EditorError, font_derived_v1::*, project_v4::*};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub struct PackageExportSourceV4 {
    pub snapshot: ProjectSnapshotV4,
    pub derived_fonts: Vec<FontDerivedSourceV1>,
}
#[derive(Clone, Debug)]
pub struct PreparedImportV4 {
    pub manifest_sha256: String,
    pub manifest: PackageManifestV4,
    pub document: ProjectDocumentV4,
}
fn invalid(message: &str) -> EditorError {
    EditorError::new("PACKAGE_INVALID", message)
}
fn id(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 1024 && !value.chars().any(char::is_control)
}
fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn bytes<T: serde::Serialize>(value: &T, limit: usize) -> Result<Vec<u8>, EditorError> {
    let bytes =
        serde_json::to_vec(value).map_err(|_| invalid("Package content cannot be serialized"))?;
    if bytes.len() > limit {
        return Err(invalid("Package content exceeds its bounded envelope"));
    }
    Ok(bytes)
}
fn digest(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}
fn require_locked(binding: &FontBindingV4) -> Result<(), EditorError> {
    if !matches!(binding, FontBindingV4::Locked { .. }) {
        return Err(EditorError::new(
            "PACKAGE_DEPENDENCY_MISSING",
            "Self-contained V4 packages require fixed fonts for text, captions and text masks",
        ));
    }
    Ok(())
}
fn validate_document(document: &ProjectDocumentV4) -> Result<(), EditorError> {
    opencut_editor_core::project_v4::validate_project_v4(document)?;
    if document.schema_version != 4 {
        return Err(invalid("V4 package requires an unprojected V4 document"));
    }
    bytes(document, 16 * 1024 * 1024)?;
    for sequence in &document.sequences {
        for caption in &sequence.captions {
            require_locked(&caption.style.font)?;
        }
        for track in &sequence.tracks {
            for element in &track.elements {
                if let ElementContentV4::Text { text } = &element.content {
                    require_locked(&text.font)?;
                }
                if let Some(visual) = &element.visual {
                    for mask in &visual.masks {
                        if let MaskShapeV4::Text { mask } = &mask.shape {
                            require_locked(&mask.font)?;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
fn validate_derived(sources: &[FontDerivedSourceV1]) -> Result<(), EditorError> {
    let mut previous: Option<&str> = None;
    for source in sources {
        let key = source.face.derivation_id.as_str();
        if previous.is_some_and(|p| p >= key) {
            return Err(invalid(
                "Derived fonts must be unique and sorted by derivation ID",
            ));
        }
        crate::font_derived_v2::compat::validate_source(source)?;
        previous = Some(key);
    }
    Ok(())
}
fn validate_proofs(
    document: &ProjectDocumentV4,
    sources: &[FontDerivedSourceV1],
) -> Result<(), EditorError> {
    validate_derived(sources)?;
    let available: BTreeMap<_, _> = sources
        .iter()
        .map(|s| (s.face.derivation_id.as_str(), s))
        .collect();
    let mut used = BTreeSet::new();
    for font in &document.fonts {
        if let ResourceOriginV4::Generated {
            provider,
            generation_id,
        } = &font.origin
        {
            if !crate::font_derived_v2::compat::is_provider(provider) {
                return Err(EditorError::new(
                    "PACKAGE_DEPENDENCY_UNSUPPORTED",
                    "Generated font has no supported portable provenance proof",
                ));
            }
            let source = available
                .get(generation_id.as_str())
                .ok_or_else(|| invalid("Derived font proof is missing"))?;
            crate::font_derived_v2::compat::validate_resource_source(font, source)?;
            used.insert(generation_id.as_str());
        }
    }
    if used.len() != available.len() {
        return Err(invalid("Package has an unreferenced derived font proof"));
    }
    Ok(())
}
fn validate_inventory(
    resources: &[PackageResourceV4],
    prefix: &str,
    max_size: u64,
) -> Result<(), EditorError> {
    let mut previous: Option<&str> = None;
    for resource in resources {
        if !id(&resource.resource_id)
            || !hash(&resource.content_hash)
            || resource.size_bytes == 0
            || resource.size_bytes > max_size
            || resource.path != format!("{prefix}/{}", resource.content_hash)
            || previous.is_some_and(|p| p >= resource.resource_id.as_str())
        {
            return Err(invalid(
                "Resource inventory is invalid, duplicated or not sorted by resource ID",
            ));
        }
        previous = Some(&resource.resource_id);
    }
    Ok(())
}
pub fn validate_export_source(source: &PackageExportSourceV4) -> Result<(), EditorError> {
    if !id(&source.snapshot.revision) {
        return Err(invalid("Export requires a fixed source revision"));
    }
    validate_document(&source.snapshot.project)?;
    validate_proofs(&source.snapshot.project, &source.derived_fonts)
}
pub fn validate_manifest(manifest: &PackageManifestV4) -> Result<(), EditorError> {
    if manifest.schema_version != 6
        || manifest.project_schema_version != 4
        || !manifest.self_contained_resources
        || manifest.history_included
        || manifest.fully_reproducible
        || !id(&manifest.source_project_id)
        || !id(&manifest.source_revision)
        || manifest.project_path != "project.json"
        || !hash(&manifest.project_sha256)
    {
        return Err(invalid("Unsupported V4 package scope or fixed identity"));
    }
    bytes(manifest, 2 * 1024 * 1024)?;
    validate_inventory(
        &manifest.assets,
        "media",
        opencut_api_contracts::uploads::MAX_UPLOAD_BYTES,
    )?;
    validate_inventory(
        &manifest.fonts,
        "fonts",
        crate::font_import_v1::MAX_FONT_BYTES_V1,
    )?;
    let mut unique = BTreeMap::new();
    for resource in manifest.assets.iter().chain(&manifest.fonts) {
        if unique
            .insert(&resource.content_hash, resource.size_bytes)
            .is_some_and(|old| old != resource.size_bytes)
        {
            return Err(invalid(
                "One content hash cannot declare different resource sizes",
            ));
        }
    }
    let total = unique
        .values()
        .try_fold(0u64, |n, size| n.checked_add(*size));
    if total.is_none_or(|n| n > 64 * 1024 * 1024 * 1024) {
        return Err(invalid("Package resource bytes exceed total limit"));
    }
    validate_derived(&manifest.derived_fonts)
}
pub fn validate_contents(
    manifest: &PackageManifestV4,
    document: &ProjectDocumentV4,
) -> Result<(), EditorError> {
    validate_manifest(manifest)?;
    validate_document(document)?;
    if document.id != manifest.source_project_id
        || digest(&bytes(document, 16 * 1024 * 1024)?) != manifest.project_sha256
    {
        return Err(invalid(
            "Package document differs from its fixed identity or digest",
        ));
    }
    let inventory = |font: bool| {
        let mut values: Vec<PackageResourceV4> = if font {
            document
                .fonts
                .iter()
                .map(|f| PackageResourceV4 {
                    resource_id: f.id.clone(),
                    content_hash: f.content.content_hash.clone(),
                    size_bytes: f.content.size,
                    path: format!("fonts/{}", f.content.content_hash),
                })
                .collect()
        } else {
            document
                .assets
                .iter()
                .map(|a| PackageResourceV4 {
                    resource_id: a.id.clone(),
                    content_hash: a.version.content_hash.clone(),
                    size_bytes: a.version.size,
                    path: format!("media/{}", a.version.content_hash),
                })
                .collect()
        };
        values.sort_by(|a, b| a.resource_id.cmp(&b.resource_id));
        values
    };
    if manifest.assets != inventory(false) || manifest.fonts != inventory(true) {
        return Err(invalid(
            "Package inventory must exactly cover every document media and font resource",
        ));
    }
    validate_proofs(document, &manifest.derived_fonts)
}
pub fn prepare_import(
    prepared: &PreparedImportV4,
    target_id: &str,
) -> Result<ProjectDocumentV4, EditorError> {
    if !id(target_id)
        || target_id.len() > 128
        || !hash(&prepared.manifest_sha256)
        || digest(&bytes(&prepared.manifest, 2 * 1024 * 1024)?) != prepared.manifest_sha256
    {
        return Err(invalid(
            "Prepared package digest or target identity is invalid",
        ));
    }
    validate_contents(&prepared.manifest, &prepared.document)?;
    let mut document = prepared.document.clone();
    document.id = target_id.into();
    validate_document(&document)?;
    Ok(document)
}
#[cfg(test)]
mod tests;
