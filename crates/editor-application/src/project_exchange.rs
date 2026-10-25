//! Complete package content uses the existing typed history validator and
//! remapper. The on-disk envelope has no development generation selector.
use crate::project_archive::{self as history, ArchiveSource, ProjectArchive};
use opencut_api_contracts::project_exchange::*;
use opencut_contracts::{EditorError, scene::*};
use std::collections::{BTreeMap, BTreeSet};

pub use history::{canonical_bytes, digest, invalid, valid_hash};
pub const MAX_PLUGIN_BYTES: u64 = 256 * 1024 * 1024;

/// A host dialog is an adapter, never a source of project or export authority.
pub trait PackageLocationPicker {
    fn choose(&self, input: &ChoosePackageLocation) -> Result<PackageLocation, EditorError>;
}

pub fn choose_location(
    caller: &crate::access::Caller,
    native: &impl PackageLocationPicker,
    input: ChoosePackageLocation,
) -> Result<PackageLocation, EditorError> {
    if !caller.is_owner() {
        return Err(crate::access::denied());
    }
    if let Some(name) = &input.suggested_name {
        if name.trim().is_empty()
            || name.len() > 240
            || name == "."
            || name == ".."
            || name.contains(['/', '\\'])
            || name.chars().any(char::is_control)
        {
            return Err(EditorError::new(
                "INVALID_ARGUMENT",
                "Choose a valid package name",
            ));
        }
    }
    native.choose(&input)
}

#[derive(Clone, Debug)]
pub struct PackageSource {
    pub content: ArchiveSource,
    pub plugin_policies: Vec<PluginPolicy>,
}

#[derive(Clone, Debug)]
pub struct PreparedPackage {
    pub manifest_sha256: String,
    pub manifest: PackageManifest,
    pub archive: ProjectArchive,
    pub tool_lock_mismatches: Vec<String>,
    pub fonts_missing: Vec<String>,
    pub install_plan: Vec<PluginGrantRestoreAction>,
}

/// The variants classify actual file content in private readers, never a public
/// protocol selector. Existing formats retain their original integrity rules.
#[derive(Clone, Debug)]
pub enum PreparedImport {
    Current(PreparedPackage),
    Media(opencut_api_contracts::project_package::PreparedImport),
    Visual(opencut_api_contracts::historical_visual_package::PreparedHistoricalVisual),
    Snapshot(crate::historical_snapshot_package::PreparedHistoricalSnapshot),
    History(history::PreparedHistoricalArchive),
}

/// One read model for the actual prepared content. Environment observations
/// come from native adapters; historical files never acquire invented locks.
pub fn inspect_prepared(prepared: &PreparedImport) -> Result<PackageInspection, EditorError> {
    let (
        hash,
        project,
        revision,
        name,
        mode,
        editable,
        assets,
        fonts,
        proofs,
        external,
        tools,
        matches,
        mismatches,
        plugins,
    ) = match prepared {
        PreparedImport::Current(p) => return inspection(p),
        PreparedImport::Media(p) => {
            crate::project_package::prepare_import(p, "package-review")?;
            let m = &p.manifest;
            (
                &p.manifest_sha256,
                &m.source_project_id,
                &m.source_revision,
                p.document.name.as_str(),
                m.mode.clone(),
                false,
                p.document.assets.len(),
                0,
                0,
                m.dependencies.fonts_not_bundled.clone(),
                vec![],
                false,
                m.dependencies.tools_not_locked.clone(),
                0,
            )
        }
        PreparedImport::Visual(p) => {
            crate::historical_visual_package::validate_manifest(&p.manifest)?;
            crate::historical_visual_package::prepare_import(p, "package-review")?;
            let m = &p.manifest;
            (
                &p.manifest_sha256,
                &m.source_project_id,
                &m.source_revision,
                p.document.base.name.as_str(),
                m.mode.clone(),
                false,
                p.document.base.assets.len(),
                0,
                0,
                m.dependencies.fonts_not_bundled.clone(),
                m.tool_locks.clone(),
                p.tool_environment_matches,
                p.tool_lock_mismatches.clone(),
                m.plugin_bundles.len(),
            )
        }
        PreparedImport::Snapshot(p) => {
            crate::historical_snapshot_package::prepare_import(p, "package-review")?;
            let m = &p.manifest;
            (
                &p.manifest_sha256,
                &m.source_project_id,
                &m.source_revision,
                p.document.name.as_str(),
                PackageMode::SelfContained,
                true,
                m.assets.len(),
                m.fonts.len(),
                m.derived_fonts.len(),
                vec![],
                vec![],
                false,
                vec!["ffmpeg".into(), "ffprobe".into()],
                0,
            )
        }
        PreparedImport::History(p) => {
            history::validate_prepared(p)?;
            let name = p
                .archive
                .revisions
                .iter()
                .find(|r| r.revision == p.manifest.source_revision)
                .ok_or_else(|| invalid("Missing historical main head"))?
                .document
                .name();
            let m = &p.manifest;
            (
                &p.manifest_sha256,
                &m.source_project_id,
                &m.source_revision,
                name,
                PackageMode::SelfContained,
                true,
                m.asset_version_count,
                m.font_version_count,
                m.derived_fonts.len(),
                vec![],
                vec![],
                false,
                vec!["ffmpeg".into(), "ffprobe".into()],
                0,
            )
        }
    };
    let mut value = PackageInspection {
        manifest_sha256: hash.clone(),
        source_project_id: project.clone(),
        source_revision: revision.clone(),
        project_name: name.to_owned(),
        authoring_available: editable,
        mode,
        scope: PackageScope::SavedRevision,
        revision_count: 1,
        transaction_count: 0,
        branch_count: 0,
        merge_count: 0,
        asset_version_count: assets,
        font_version_count: fonts,
        derived_font_count: proofs,
        plugin_bundle_count: plugins,
        external_font_families: external,
        fonts_missing: vec![],
        tool_locks: tools,
        tool_environment_matches: matches,
        tool_lock_mismatches: mismatches,
    };
    if let PreparedImport::Visual(p) = prepared {
        value.fonts_missing = p.fonts_missing.clone();
    }
    if let PreparedImport::History(p) = prepared {
        value.scope = PackageScope::History;
        value.revision_count = p.manifest.revision_count;
        value.transaction_count = p.manifest.transaction_count;
        value.branch_count = p.manifest.branch_count;
        value.merge_count = p.manifest.merge_count;
    }
    Ok(value)
}

pub fn prepare_historical_archive(
    p: &history::PreparedHistoricalArchive,
    target: &str,
) -> Result<ProjectArchive, EditorError> {
    history::validate_prepared(p)?;
    history::remap_archive(
        &p.archive,
        &p.manifest.derived_fonts,
        &p.manifest_sha256,
        target,
        "project-package",
        true,
    )
}
/// Only the envelope and current documents omit their internal storage tags.
/// Causal journals and immutable provenance retain original identity bytes.
pub fn archive_bytes(archive: &ProjectArchive) -> Result<Vec<u8>, EditorError> {
    let mut value =
        serde_json::to_value(archive).map_err(|_| invalid("Cannot encode package archive"))?;
    value
        .as_object_mut()
        .ok_or_else(|| invalid("Invalid archive envelope"))?
        .remove("schema_version");
    for row in value["revisions"]
        .as_array_mut()
        .ok_or_else(|| invalid("Missing package revisions"))?
    {
        let document = row["document"]
            .as_object_mut()
            .ok_or_else(|| invalid("Invalid package document"))?;
        if document
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            == Some(4)
        {
            document.remove("schema_version");
        }
    }
    canonical_bytes(&value, history::MAX_HISTORY_BYTES)
}

pub fn decode_archive(bytes: &[u8]) -> Result<ProjectArchive, EditorError> {
    if bytes.len() > history::MAX_HISTORY_BYTES {
        return Err(invalid("Package archive exceeds JSON budget"));
    }
    let mut value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| invalid("Invalid package archive JSON"))?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| invalid("Invalid package archive"))?;
    if object.insert("schema_version".into(), 1.into()).is_some() {
        return Err(invalid("Package archive contains a development selector"));
    }
    for row in value["revisions"]
        .as_array_mut()
        .ok_or_else(|| invalid("Missing package revisions"))?
    {
        let document = row["document"]
            .as_object_mut()
            .ok_or_else(|| invalid("Invalid package document"))?;
        match document.get("schema_version") {
            None => {
                document.insert("schema_version".into(), 4.into());
            }
            Some(value) if matches!(value.as_u64(), Some(1 | 2 | 3 | 5)) => {}
            _ => {
                return Err(invalid(
                    "Package document contains an unknown or development selector",
                ));
            }
        }
    }
    let archive =
        serde_json::from_value(value).map_err(|_| invalid("Unknown package archive fields"))?;
    if archive_bytes(&archive)? != bytes {
        return Err(invalid("Package archive is not canonical"));
    }
    Ok(archive)
}

pub fn policy_identity(policy: &PluginPolicy) -> (PluginKind, &str, &str, &str, &str) {
    match policy {
        PluginPolicy::Wasm(g) => (
            PluginKind::Wasm,
            &g.target.id,
            &g.target.version,
            &g.package_sha256,
            &g.target.project_id,
        ),
        PluginPolicy::Presets(g) => (
            PluginKind::Presets,
            &g.target.id,
            &g.target.version,
            &g.package_sha256,
            &g.target.project_id,
        ),
    }
}

pub fn grant_plan(
    policies: &[PluginPolicy],
    target: &str,
    hash: &str,
) -> Result<Vec<PluginGrantRestoreAction>, EditorError> {
    if !history::valid_id(target) || !valid_hash(hash) {
        return Err(invalid("Invalid restore proposal identity"));
    }
    policies
        .iter()
        .enumerate()
        .map(|(index, policy)| {
            let key = format!("package-restore-{hash}-{index}");
            match policy {
                PluginPolicy::Wasm(g) => {
                    let request = opencut_api_contracts::project_plugins::SetGrant {
                        target: opencut_api_contracts::project_plugins::PluginTarget {
                            project_id: target.into(),
                            id: g.target.id.clone(),
                            version: g.target.version.clone(),
                        },
                        package_sha256: g.package_sha256.clone(),
                        operations: g.operations.clone(),
                        enabled: g.enabled,
                        expected_revision: "0".into(),
                        idempotency_key: key,
                    };
                    crate::project_plugins::validate_grant(&request)?;
                    Ok(PluginGrantRestoreAction::WasmGrant(request))
                }
                PluginPolicy::Presets(g) => {
                    let request = opencut_api_contracts::presets::SetGrant {
                        target: opencut_api_contracts::presets::GrantTarget {
                            project_id: target.into(),
                            id: g.target.id.clone(),
                            version: g.target.version.clone(),
                        },
                        package_sha256: g.package_sha256.clone(),
                        preset_names: g.preset_names.clone(),
                        enabled: g.enabled,
                        expected_revision: "0".into(),
                        idempotency_key: key,
                    };
                    crate::presets::validate_grant(&request)?;
                    Ok(PluginGrantRestoreAction::PresetGrant(request))
                }
            }
        })
        .collect()
}

pub fn external_fonts(archive: &ProjectArchive) -> Vec<String> {
    let mut families: BTreeSet<_> = archive
        .revisions
        .iter()
        .flat_map(|r| r.document.external_fonts())
        .collect();
    let mut observe = |font: &FontBinding| {
        if let FontBinding::UnresolvedFamily { family, .. } = font {
            families.insert(family.clone());
        }
    };
    for r in &archive.revisions {
        let Some(document) = r.document.editor_content() else {
            continue;
        };
        for sequence in &document.sequences {
            for caption in &sequence.captions {
                observe(&caption.style.font);
            }
            for element in sequence.tracks.iter().flat_map(|t| &t.elements) {
                if let ElementContent::Text { text } = &element.content {
                    observe(&text.font);
                }
                if let Some(visual) = &element.visual {
                    for mask in &visual.masks {
                        if let LayerMaskShape::Text { mask } = &mask.shape {
                            observe(&mask.font);
                        }
                    }
                }
            }
        }
    }
    families.into_iter().collect()
}

pub fn validate_source(
    source: &PackageSource,
    scope: PackageScope,
    mode: &PackageMode,
) -> Result<(), EditorError> {
    history::validate_content_for_exchange(&source.content, *mode == PackageMode::SelfContained)?;
    let a = &source.content.archive;
    if scope == PackageScope::SavedRevision
        && (a.revisions.len() != 1
            || !a.transactions.is_empty()
            || !a.branches.is_empty()
            || !a.merges.is_empty())
    {
        return Err(invalid(
            "Saved revision package must contain exactly one root",
        ));
    }
    if source.plugin_policies.len() > 128 {
        return Err(invalid("Package contains too many plugin policies"));
    }
    let mut previous = None;
    for policy in &source.plugin_policies {
        let (kind, id, version, hash, project) = policy_identity(policy);
        let identity = (kind, id, version);
        if project != a.source_project_id
            || !valid_hash(hash)
            || previous.is_some_and(|p| p >= identity)
        {
            return Err(invalid(
                "Plugin policy identity is inconsistent or not ordered",
            ));
        }
        previous = Some(identity);
    }
    grant_plan(
        &source.plugin_policies,
        &a.source_project_id,
        &"0".repeat(64),
    )?;
    Ok(())
}

pub fn make_manifest(
    source: &PackageSource,
    scope: PackageScope,
    mode: PackageMode,
    blobs: Vec<PackageBlob>,
    bundles: Vec<PluginBundle>,
    tools: Vec<PackageToolLock>,
) -> Result<PackageManifest, EditorError> {
    validate_source(source, scope, &mode)?;
    let a = &source.content.archive;
    let (_, assets, fonts) = history::content_inventory(a)?;
    let manifest = PackageManifest {
        source_project_id: a.source_project_id.clone(),
        source_revision: a.main_head_revision.clone(),
        scope,
        mode,
        archive_path: "archive.json".into(),
        archive_sha256: digest(&archive_bytes(a)?),
        revision_count: a.revisions.len(),
        transaction_count: a.transactions.len(),
        branch_count: a.branches.len(),
        merge_count: a.merges.len(),
        asset_version_count: assets,
        font_version_count: fonts,
        blobs,
        derived_fonts: source.content.derived_fonts.clone(),
        external_font_families: external_fonts(a),
        tool_locks: tools,
        plugin_policies: source.plugin_policies.clone(),
        plugin_bundles: bundles,
    };
    validate_manifest(&manifest, a)?;
    Ok(manifest)
}

pub fn validate_manifest(m: &PackageManifest, a: &ProjectArchive) -> Result<(), EditorError> {
    let source = PackageSource {
        content: ArchiveSource {
            archive: a.clone(),
            derived_fonts: m.derived_fonts.clone(),
        },
        plugin_policies: m.plugin_policies.clone(),
    };
    validate_source(&source, m.scope, &m.mode)?;
    let (blobs, assets, fonts) = history::content_inventory(a)?;
    if m.source_project_id != a.source_project_id
        || m.source_revision != a.main_head_revision
        || m.archive_path != "archive.json"
        || m.archive_sha256 != digest(&archive_bytes(a)?)
        || (
            m.revision_count,
            m.transaction_count,
            m.branch_count,
            m.merge_count,
            m.asset_version_count,
            m.font_version_count,
        ) != (
            a.revisions.len(),
            a.transactions.len(),
            a.branches.len(),
            a.merges.len(),
            assets,
            fonts,
        )
        || m.external_font_families != external_fonts(a)
        || m.blobs.len() != blobs.len()
    {
        return Err(invalid(
            "Manifest does not exactly describe package content",
        ));
    }
    for (declared, actual) in m.blobs.iter().zip(blobs) {
        if declared.content_hash != actual.content_hash || declared.size_bytes != actual.size_bytes
        {
            return Err(invalid("Package resource closure differs"));
        }
        match (&m.mode, &declared.location) {
            (PackageMode::SelfContained, BlobLocation::Bundled(path)) if *path == actual.path => (),
            (PackageMode::Linked, BlobLocation::Linked(path))
                if path.starts_with('/')
                    && !path.split('/').any(|p| p == "..")
                    && !path.chars().any(char::is_control) =>
            {
                ()
            }
            _ => {
                return Err(invalid(
                    "Resource location differs from package storage mode",
                ));
            }
        }
    }
    if m.plugin_bundles.len() != m.plugin_policies.len() {
        return Err(invalid("Package omits plugin dependencies"));
    }
    for (bundle, policy) in m.plugin_bundles.iter().zip(&m.plugin_policies) {
        let (kind, id, version, hash, _) = policy_identity(policy);
        if (
            bundle.kind,
            bundle.id.as_str(),
            bundle.version.as_str(),
            bundle.package_sha256.as_str(),
        ) != (kind, id, version, hash)
            || bundle.path
                != format!(
                    "plugins/{}/{hash}",
                    match kind {
                        PluginKind::Wasm => "wasm",
                        PluginKind::Presets => "presets",
                    }
                )
            || (kind == PluginKind::Presets && !bundle.proposed_global_permissions.is_empty())
        {
            return Err(invalid("Plugin bundle differs from policy identity"));
        }
    }
    let mut previous = None;
    for tool in &m.tool_locks {
        if !["ffmpeg", "ffprobe"].contains(&tool.name.as_str())
            || !valid_hash(&tool.binary_sha256)
            || !history::valid_id(&tool.version)
            || previous.is_some_and(|v| v >= tool.name.as_str())
        {
            return Err(invalid("Invalid native tool lock inventory"));
        }
        previous = Some(tool.name.as_str());
    }

    canonical_bytes(m, history::MAX_MANIFEST_BYTES)?;
    Ok(())
}

pub fn validate_prepared(p: &PreparedPackage) -> Result<(), EditorError> {
    if !valid_hash(&p.manifest_sha256)
        || digest(&canonical_bytes(&p.manifest, history::MAX_MANIFEST_BYTES)?) != p.manifest_sha256
    {
        return Err(invalid("Package manifest identity differs"));
    }
    validate_manifest(&p.manifest, &p.archive)
}

pub fn prepare_import(p: &PreparedPackage, target: &str) -> Result<ProjectArchive, EditorError> {
    validate_prepared(p)?;
    history::remap_archive(
        &p.archive,
        &p.manifest.derived_fonts,
        &p.manifest_sha256,
        target,
        "project-package",
        p.manifest.mode == PackageMode::SelfContained,
    )
}

pub fn inspection(p: &PreparedPackage) -> Result<PackageInspection, EditorError> {
    validate_prepared(p)?;
    let m = &p.manifest;
    let main = p
        .archive
        .revisions
        .iter()
        .find(|r| r.revision == m.source_revision)
        .ok_or_else(|| invalid("Missing package main head"))?;
    Ok(PackageInspection {
        manifest_sha256: p.manifest_sha256.clone(),
        source_project_id: m.source_project_id.clone(),
        source_revision: m.source_revision.clone(),
        project_name: main.document.name().to_owned(),
        authoring_available: matches!(&main.document, history::ArchivedDocument::Editable(_)),
        mode: m.mode.clone(),
        scope: m.scope,
        revision_count: m.revision_count,
        transaction_count: m.transaction_count,
        branch_count: m.branch_count,
        merge_count: m.merge_count,
        asset_version_count: m.asset_version_count,
        font_version_count: m.font_version_count,
        derived_font_count: m.derived_fonts.len(),
        plugin_bundle_count: m.plugin_bundles.len(),
        external_font_families: m.external_font_families.clone(),
        fonts_missing: p.fonts_missing.clone(),
        tool_locks: m.tool_locks.clone(),
        tool_environment_matches: p.tool_lock_mismatches.is_empty(),
        tool_lock_mismatches: p.tool_lock_mismatches.clone(),
    })
}

pub fn blob_versions(
    archive: &ProjectArchive,
) -> Result<BTreeMap<String, opencut_contracts::AssetVersion>, EditorError> {
    Ok(history::content_inventory(archive)?
        .0
        .into_iter()
        .map(|b| {
            (
                b.content_hash.clone(),
                opencut_contracts::AssetVersion {
                    id: format!("av_{}", b.content_hash),
                    content_hash: b.content_hash,
                    size: b.size_bytes,
                },
            )
        })
        .collect())
}

#[cfg(test)]
mod location_tests {
    use super::*;
    use crate::access::{Caller, VerifiedCredential};
    use std::cell::Cell;
    struct Picker(Cell<usize>);
    impl PackageLocationPicker for Picker {
        fn choose(&self, _: &ChoosePackageLocation) -> Result<PackageLocation, EditorError> {
            self.0.set(self.0.get() + 1);
            Ok(PackageLocation { path: None })
        }
    }
    fn request(name: &str) -> ChoosePackageLocation {
        ChoosePackageLocation {
            purpose: PackageLocationPurpose::Save,
            suggested_name: Some(name.into()),
            language: PackageDialogLanguage::Zh,
        }
    }
    #[test]
    fn authority_and_name_validation_precede_opening_a_native_dialog() {
        let picker = Picker(Cell::new(0));
        let caller = Caller::Credential(VerifiedCredential::from_verified(
            "agent".into(),
            "credential".into(),
        ));
        assert_eq!(
            choose_location(&caller, &picker, request("clip"))
                .unwrap_err()
                .code,
            "NOT_AUTHORIZED"
        );
        for name in ["", "..", "one/two", "one\\two", "one\nname"] {
            assert!(choose_location(&Caller::LocalOwner, &picker, request(name)).is_err());
        }
        assert_eq!(picker.0.get(), 0);
        assert!(
            choose_location(&Caller::LocalOwner, &picker, request("旅行剪辑.opencut"))
                .unwrap()
                .path
                .is_none()
        );
        assert_eq!(picker.0.get(), 1);
    }
}
