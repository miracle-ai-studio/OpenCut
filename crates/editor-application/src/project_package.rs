//! Portable package validation and dependency accounting; no native path access.
use crate::{content_id, encode_json};
use opencut_api_contracts::project_package::{
    PackageDependencies, PackageManifest, PreparedImport,
};
use opencut_contracts::{EditorError, FontResolutionPolicy, ProjectDocument, ProjectSnapshot};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub struct PackageExportSource {
    pub snapshot: ProjectSnapshot,
    /// Project-scoped grant identities reported even though packages do not install plugins.
    pub plugin_grants: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct PreparedDocument {
    pub document: ProjectDocument,
    pub revision: String,
}

fn invalid(message: &str) -> EditorError {
    EditorError::new("PACKAGE_INVALID", message)
}

pub fn dependencies(source: &PackageExportSource) -> PackageDependencies {
    let mut fonts = BTreeSet::new();
    for sequence in &source.snapshot.project.sequences {
        for cue in &sequence.caption_cues {
            fonts.insert(cue.style.font_family.clone());
            if let FontResolutionPolicy::ExplicitFallback { families } = &cue.style.font_policy {
                fonts.extend(families.iter().cloned());
            }
        }
    }
    let mut plugins = source.plugin_grants.clone();
    plugins.sort();
    plugins.dedup();
    PackageDependencies {
        fonts_not_bundled: fonts.into_iter().collect(),
        plugins_not_bundled: plugins,
        tools_not_locked: vec!["ffmpeg".into(), "ffprobe".into()],
        unsupported: Vec::new(),
    }
}

pub fn validate_export_document(document: &ProjectDocument) -> Result<(), EditorError> {
    opencut_editor_core::validate_project(document)
}

pub fn prepare_import(
    prepared: &PreparedImport,
    target_project_id: &str,
) -> Result<PreparedDocument, EditorError> {
    let manifest: &PackageManifest = &prepared.manifest;
    if manifest.schema_version != 1
        || target_project_id.trim().is_empty()
        || manifest.source_project_id != prepared.document.id
        || !manifest.dependencies.unsupported.is_empty()
    {
        return Err(invalid("Unsupported package schema, target, or dependency"));
    }
    // Grant state is separate from the project document. Importing without it
    // would silently change behavior, so such a package needs a later installer.
    if !manifest.dependencies.plugins_not_bundled.is_empty() {
        return Err(EditorError::new(
            "PACKAGE_DEPENDENCY_MISSING",
            "Project plugin grants require an explicit installation workflow",
        ));
    }
    let listed: BTreeMap<_, _> = manifest
        .assets
        .iter()
        .map(|asset| (asset.asset_id.as_str(), asset))
        .collect();
    if listed.len() != manifest.assets.len() || listed.len() != prepared.document.assets.len() {
        return Err(invalid(
            "Package asset inventory is incomplete or duplicated",
        ));
    }
    let mut document = prepared.document.clone();
    for asset in &mut document.assets {
        let listed = listed
            .get(asset.id.as_str())
            .ok_or_else(|| invalid("Package asset inventory omits a project asset"))?;
        let version = asset
            .version
            .as_ref()
            .ok_or_else(|| invalid("Package asset lacks a fixed content version"))?;
        if version.id != format!("av_{}", listed.content_hash)
            || version.content_hash != listed.content_hash
            || version.size != listed.size
        {
            return Err(invalid("Package asset version differs from inventory"));
        }
        asset.uri = format!("opencut-package://sha256/{}", listed.content_hash);
    }
    document.id = target_project_id.into();
    opencut_editor_core::validate_project(&document)?;
    let material = encode_json(&document, "imported project document")?;
    let revision = content_id("rev", material.as_bytes());
    Ok(PreparedDocument { document, revision })
}
