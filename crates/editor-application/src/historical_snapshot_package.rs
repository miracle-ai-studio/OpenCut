//! Pure single-revision V4 package validation. Native files and prepared import
//! authority stay outside public DTOs; every declared resource is accounted for.
use opencut_api_contracts::historical_snapshot_package::{
    HistoricalSnapshotManifest, HistoricalSnapshotResource,
};
use opencut_contracts::{EditorError, font_derived::*, scene::*};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub struct HistoricalSnapshotSource {
    pub snapshot: EditorSnapshot,
    pub derived_fonts: Vec<FontDerivedSource>,
}
#[derive(Clone, Debug)]
pub struct PreparedHistoricalSnapshot {
    pub manifest_sha256: String,
    pub manifest: HistoricalSnapshotManifest,
    pub document: EditorDocument,
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
fn require_locked(binding: &FontBinding) -> Result<(), EditorError> {
    if !matches!(binding, FontBinding::Locked { .. }) {
        return Err(EditorError::new(
            "PACKAGE_DEPENDENCY_MISSING",
            "Self-contained packages require fixed fonts for text, captions and text masks",
        ));
    }
    Ok(())
}
fn validate_document(
    document: &EditorDocument,
    require_bundled_fonts: bool,
) -> Result<(), EditorError> {
    opencut_editor_core::scene::validate_project_scene(document)?;
    if document.schema_version != 4 {
        return Err(invalid("Package requires a complete editor document"));
    }
    bytes(document, 16 * 1024 * 1024)?;
    if !require_bundled_fonts {
        return Ok(());
    }
    for sequence in &document.sequences {
        for caption in &sequence.captions {
            require_locked(&caption.style.font)?;
        }
        for track in &sequence.tracks {
            for element in &track.elements {
                if let ElementContent::Text { text } = &element.content {
                    require_locked(&text.font)?;
                }
                if let Some(visual) = &element.visual {
                    for mask in &visual.masks {
                        if let LayerMaskShape::Text { mask } = &mask.shape {
                            require_locked(&mask.font)?;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
fn validate_derived(sources: &[FontDerivedSource]) -> Result<(), EditorError> {
    let mut previous: Option<&str> = None;
    for source in sources {
        let key = source.face.derivation_id.as_str();
        if previous.is_some_and(|p| p >= key) {
            return Err(invalid(
                "Derived fonts must be unique and sorted by derivation ID",
            ));
        }
        crate::font_derived::provenance::validate_source(source)?;
        previous = Some(key);
    }
    Ok(())
}
fn validate_proofs(
    document: &EditorDocument,
    sources: &[FontDerivedSource],
) -> Result<(), EditorError> {
    validate_derived(sources)?;
    let available: BTreeMap<_, _> = sources
        .iter()
        .map(|s| (s.face.derivation_id.as_str(), s))
        .collect();
    let mut used = BTreeSet::new();
    for font in &document.fonts {
        if let ResourceOrigin::Generated {
            provider,
            generation_id,
        } = &font.origin
        {
            if !crate::font_derived::provenance::is_provider(provider) {
                return Err(EditorError::new(
                    "PACKAGE_DEPENDENCY_UNSUPPORTED",
                    "Generated font has no supported portable provenance proof",
                ));
            }
            let source = available
                .get(generation_id.as_str())
                .ok_or_else(|| invalid("Derived font proof is missing"))?;
            crate::font_derived::provenance::validate_resource_source(font, source)?;
            used.insert(generation_id.as_str());
        }
    }
    if used.len() != available.len() {
        return Err(invalid("Package has an unreferenced derived font proof"));
    }
    Ok(())
}
fn validate_inventory(
    resources: &[HistoricalSnapshotResource],
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
pub fn validate_export_source(source: &HistoricalSnapshotSource) -> Result<(), EditorError> {
    if !id(&source.snapshot.revision) {
        return Err(invalid("Export requires a fixed source revision"));
    }
    validate_export_content(source, true)
}
pub(crate) fn validate_export_content(
    source: &HistoricalSnapshotSource,
    require_bundled_fonts: bool,
) -> Result<(), EditorError> {
    validate_document(&source.snapshot.project, require_bundled_fonts)?;
    validate_proofs(&source.snapshot.project, &source.derived_fonts)
}
pub fn validate_manifest(manifest: &HistoricalSnapshotManifest) -> Result<(), EditorError> {
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
        16 * 1024 * 1024 * 1024, // Exact historical package format bound.
    )?;
    validate_inventory(&manifest.fonts, "fonts", crate::font_import::MAX_FONT_BYTES)?;
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
    manifest: &HistoricalSnapshotManifest,
    document: &EditorDocument,
) -> Result<(), EditorError> {
    validate_manifest(manifest)?;
    validate_document(document, true)?;
    if document.id != manifest.source_project_id
        || digest(&bytes(document, 16 * 1024 * 1024)?) != manifest.project_sha256
    {
        return Err(invalid(
            "Package document differs from its fixed identity or digest",
        ));
    }
    let inventory = |font: bool| {
        let mut values: Vec<HistoricalSnapshotResource> = if font {
            document
                .fonts
                .iter()
                .map(|f| HistoricalSnapshotResource {
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
                .map(|a| HistoricalSnapshotResource {
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
    prepared: &PreparedHistoricalSnapshot,
    target_id: &str,
) -> Result<EditorDocument, EditorError> {
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
    validate_document(&document, true)?;
    Ok(document)
}
#[cfg(test)]
mod tests;
