//! Scoped immutable resources and read-only CAS verification before SQL writes.
use super::*;
use opencut_contracts::{Asset, AssetKind, AssetVersion, FrameRate, ProjectDocument, Sequence};
use opencut_editor_application::gc::{
    MAX_REFERENCE_BYTES, MAX_REFERENCE_DOCUMENT_BYTES, MAX_REFERENCE_MS, MAX_REFERENCE_ROWS,
};
use opencut_editor_core::project_v4::legacy::*;
use std::{collections::HashSet, path::Path};

#[derive(Default)]
struct ScanBudget {
    bytes: usize,
    rows: usize,
}
impl ScanBudget {
    fn observe(&mut self, raw: &str, start: std::time::Instant) -> Result<(), EditorError> {
        self.bytes = self.bytes.saturating_add(raw.len());
        self.rows += 1;
        if self.bytes > MAX_REFERENCE_BYTES
            || raw.len() > MAX_REFERENCE_DOCUMENT_BYTES
            || self.rows > MAX_REFERENCE_ROWS
            || start.elapsed() > Duration::from_millis(MAX_REFERENCE_MS)
        {
            return Err(EditorError::new(
                "RESOURCE_TOO_LARGE",
                "V4 resource-scope scan exceeded its bounded reference budget",
            ));
        }
        Ok(())
    }
}

pub(super) fn verify_document(
    root: &Path,
    document: &ProjectDocumentV4,
    inherited: Option<&StoredProjectDocumentV4>,
) -> Result<AssetStore, EditorError> {
    let store = AssetStore::read_only(root)?;
    let mut existing = HashSet::new();
    match inherited {
        Some(StoredProjectDocumentV4::V4(value)) => {
            existing.extend(
                value
                    .assets
                    .iter()
                    .map(|a| (a.version.content_hash.as_str(), a.version.size)),
            );
            existing.extend(
                value
                    .fonts
                    .iter()
                    .map(|f| (f.content.content_hash.as_str(), f.content.size)),
            );
        }
        Some(StoredProjectDocumentV4::Legacy(value)) => existing.extend(
            value
                .assets
                .iter()
                .filter_map(|a| a.version.as_ref())
                .map(|v| (v.content_hash.as_str(), v.size)),
        ),
        Some(StoredProjectDocumentV4::V3(value)) => existing.extend(
            value
                .base
                .assets
                .iter()
                .filter_map(|a| a.version.as_ref())
                .map(|v| (v.content_hash.as_str(), v.size)),
        ),
        None => {}
    }
    let mut checked = HashSet::new();
    for version in document
        .assets
        .iter()
        .map(|a| &a.version)
        .chain(document.fonts.iter().map(|f| &f.content))
    {
        // Immutable binding identity is independent of editor-owned logical ID.
        // Existing unavailable media remain editable; execution rechecks bytes.
        if existing.contains(&(version.content_hash.as_str(), version.size)) {
            continue;
        }
        if checked.insert((version.content_hash.as_str(), version.size)) {
            // Only the CAS version is consumed. The URI is deliberately absent:
            // an owner-supplied host path never becomes an implicit capture.
            let asset = Asset {
                id: version.id.clone(),
                name: "Pinned V4 resource".into(),
                kind: AssetKind::Graphic,
                uri: String::new(),
                duration: None,
                version: Some(version.clone()),
                metadata: Default::default(),
            };
            store.open_verified(&asset)?;
        }
    }
    Ok(store)
}

struct ResourceScope {
    assets: Vec<AssetV4>,
    fonts: Vec<FontResourceV4>,
    legacy: Vec<Asset>,
}
impl ResourceScope {
    fn load(connection: &Connection, project: &str) -> Result<Self, EditorError> {
        let mut result = Self {
            assets: Vec::new(),
            fonts: Vec::new(),
            legacy: Vec::new(),
        };
        let start = std::time::Instant::now();
        let mut budget = ScanBudget::default();
        let mut statement = connection
            .prepare("SELECT document_json FROM revisions WHERE project_id=?1")
            .map_err(storage_error)?;
        let rows = statement
            .query_map([project], |row| row.get::<_, String>(0))
            .map_err(storage_error)?;
        for row in rows {
            let raw = row.map_err(storage_error)?;
            budget.observe(&raw, start)?;
            let document = decode_versioned_project(&raw)?;
            let id = match &document {
                VersionedProjectDocument::Legacy(value) => &value.id,
                VersionedProjectDocument::V3(value) => &value.base.id,
                VersionedProjectDocument::V4(value) => &value.id,
            };
            if id != project {
                return Err(EditorError::new(
                    "STORAGE_ERROR",
                    "Historical resource document does not match its project scope",
                ));
            }
            match document {
                VersionedProjectDocument::Legacy(value) => result.legacy.extend(value.assets),
                VersionedProjectDocument::V3(value) => result.legacy.extend(value.base.assets),
                VersionedProjectDocument::V4(value) => {
                    result.assets.extend(value.assets);
                    result.fonts.extend(value.fonts);
                }
            }
        }
        let mut statement = connection.prepare("SELECT m.asset_json FROM available_project_media a JOIN project_media m USING(media_id) WHERE a.project_id=?1").map_err(storage_error)?;
        let rows = statement
            .query_map([project], |row| row.get::<_, String>(0))
            .map_err(storage_error)?;
        for row in rows {
            let raw = row.map_err(storage_error)?;
            budget.observe(&raw, start)?;
            result
                .legacy
                .push(decode_json(&raw, "Scoped project media")?);
        }
        if start.elapsed() > Duration::from_millis(MAX_REFERENCE_MS) {
            return Err(EditorError::new(
                "RESOURCE_TOO_LARGE",
                "V4 resource-scope scan exceeded its bounded reference budget",
            ));
        }
        Ok(result)
    }
    fn current(connection: &Connection, project: &str) -> Result<Self, EditorError> {
        let source = load_source(connection, project, None)?;
        let mut result = Self {
            assets: Vec::new(),
            fonts: Vec::new(),
            legacy: Vec::new(),
        };
        match source.document {
            StoredProjectDocumentV4::Legacy(value) => result.legacy = value.assets,
            StoredProjectDocumentV4::V3(value) => result.legacy = value.base.assets,
            StoredProjectDocumentV4::V4(value) => {
                result.assets = value.assets;
                result.fonts = value.fonts;
            }
        }
        Ok(result)
    }
    fn asset(&self, candidate: &AssetV4) -> bool {
        self.assets.iter().any(|asset| same_asset(asset, candidate))
            || self.legacy.iter().any(|asset| {
                known_version(asset).is_some_and(|version| version == candidate.version)
                    && canonical_asset(asset, &candidate.version)
                        .is_some_and(|asset| same_asset(&asset, candidate))
            })
    }
    fn version(&self, version: &AssetVersion) -> bool {
        self.assets.iter().any(|asset| &asset.version == version)
            || self.fonts.iter().any(|font| &font.content == version)
            || self
                .legacy
                .iter()
                .any(|asset| known_version(asset).as_ref() == Some(version))
    }
    fn font(&self, candidate: &FontResourceV4) -> bool {
        self.fonts.iter().any(|font| {
            let mut font = font.clone();
            font.id = candidate.id.clone();
            font == *candidate
        })
    }
}

fn same_asset(authority: &AssetV4, candidate: &AssetV4) -> bool {
    let mut authority = authority.clone();
    // Display identity is editor-owned; bytes, classification, observation,
    // provenance and license cannot be forged by a restricted client.
    authority.id = candidate.id.clone();
    authority.name = candidate.name.clone();
    authority == *candidate
}
pub(super) fn known_version(asset: &Asset) -> Option<AssetVersion> {
    asset.version.clone().or_else(|| {
        let hash = asset.metadata.get("content_hash")?.as_str()?;
        let size = asset.metadata.get("size")?.as_u64()?;
        Some(AssetVersion {
            id: format!("av_{hash}"),
            content_hash: hash.into(),
            size,
        })
    })
}

/// Reuse the one pure legacy observation projection instead of hand-copying
/// another metadata interpretation into the authorization adapter.
pub(super) fn canonical_asset(asset: &Asset, version: &AssetVersion) -> Option<AssetV4> {
    let observation = packet_rate(asset).ok()?;
    let mut asset = asset.clone();
    asset
        .metadata
        .remove(opencut_media_contracts::CLASSIC_PACKET_RATE_METADATA_KEY_V1);
    if let Some(observation) = observation {
        // Preserve the persisted FFprobe header. Only the V4 projection uses
        // Classic's normalized packet rate; explicit unknown stays unknown.
        asset.metadata.remove("frame_rate");
        if let Some(rate) = observation.average_packet_rate {
            let fps = opencut_editor_application::media_import::normalize_classic_packet_fps(rate)
                .ok()?;
            asset
                .metadata
                .insert("frame_rate".into(), serde_json::to_value(fps).ok()?);
        }
    }
    let mut project_id = "v4_scope_project".to_string();
    while project_id == asset.id {
        project_id.push('_');
    }
    let mut sequence_id = "v4_scope_scene".to_string();
    while sequence_id == asset.id || sequence_id == project_id {
        sequence_id.push('_');
    }
    let project = ProjectDocument {
        id: project_id,
        name: "Resource projection".into(),
        schema_version: 2,
        default_sequence_id: sequence_id.clone(),
        assets: vec![asset.clone()],
        sequences: vec![Sequence {
            id: sequence_id,
            name: "Projection".into(),
            width: 1,
            height: 1,
            fps: FrameRate {
                numerator: 1,
                denominator: 1,
            },
            tracks: Vec::new(),
            caption_cues: Vec::new(),
        }],
    };
    let inputs = LegacyConversionInputs {
        asset_versions: std::collections::BTreeMap::from([(asset.id.clone(), version.clone())]),
    };
    match convert_legacy_project(LegacyProjectInput::Base(&project), &inputs) {
        LegacyConversionResult::Converted { mut project, .. } => project.assets.pop(),
        LegacyConversionResult::Rejected { .. } => None,
    }
}

pub(super) fn packet_rate(
    asset: &Asset,
) -> Result<Option<opencut_media_contracts::ClassicPacketRateV1>, EditorError> {
    let Some(value) = asset
        .metadata
        .get(opencut_media_contracts::CLASSIC_PACKET_RATE_METADATA_KEY_V1)
    else {
        return Ok(None);
    };
    let invalid = || {
        EditorError::new(
            "INVALID_MEDIA_OBSERVATION",
            "Invalid Classic packet-rate observation",
        )
    };
    if asset.kind != opencut_contracts::AssetKind::Video {
        return Err(invalid());
    }
    let observation: opencut_media_contracts::ClassicPacketRateV1 =
        serde_json::from_value(value.clone()).map_err(|_| invalid())?;
    observation.validate().map_err(|_| invalid())?;
    Ok(Some(observation))
}

pub(super) fn authorize_intent(
    connection: &Connection,
    context: &opencut_editor_application::audit::ExecutionContext,
    intent: V4Intent<'_>,
) -> Result<(), EditorError> {
    if let V4Intent::PublicApply(input) = intent {
        return super::public::authorize_bindings(connection, context, input);
    }
    if matches!(intent, V4Intent::Undo(_) | V4Intent::Redo(_)) {
        return Ok(());
    }
    if context.caller().is_owner() {
        return Ok(());
    }
    if let V4Intent::Apply(input) = intent
        && input.operations.iter().all(|operation| {
            !matches!(
                operation,
                ProjectOperationV4::AssetPut { .. } | ProjectOperationV4::FontPut { .. }
            )
        })
    {
        return Ok(());
    }
    let scope = ResourceScope::load(connection, intent.project_id())?;
    match intent {
        V4Intent::PublicApply(_) | V4Intent::Undo(_) | V4Intent::Redo(_) => {
            unreachable!("handled above")
        }
        V4Intent::Create(_) | V4Intent::Copy(_) => Err(access::denied()),
        V4Intent::Apply(input) => {
            for operation in &input.operations {
                match operation {
                    ProjectOperationV4::AssetPut { asset } if !scope.asset(asset) => {
                        return Err(access::denied());
                    }
                    ProjectOperationV4::FontPut { font } if !scope.font(font) => {
                        return Err(access::denied());
                    }
                    _ => {}
                }
            }
            Ok(())
        }
        V4Intent::Promote(input) => {
            if input
                .asset_versions
                .values()
                .any(|version| !scope.version(version))
            {
                return Err(access::denied());
            }
            Ok(())
        }
        V4Intent::Restore(input) => {
            if input
                .asset_versions
                .values()
                .any(|version| !scope.version(version))
            {
                return Err(access::denied());
            }
            Ok(())
        }
    }
}

pub(super) fn authorize_document(
    connection: &Connection,
    context: &opencut_editor_application::audit::ExecutionContext,
    project: &str,
    document: &ProjectDocumentV4,
) -> Result<(), EditorError> {
    if context.caller().is_owner() {
        return Ok(());
    }
    let current = ResourceScope::current(connection, project)?;
    if document.assets.iter().all(|asset| current.asset(asset))
        && document.fonts.iter().all(|font| current.font(font))
    {
        return Ok(());
    }
    let scope = ResourceScope::load(connection, project)?;
    if document.assets.iter().any(|asset| !scope.asset(asset))
        || document.fonts.iter().any(|font| !scope.font(font))
    {
        return Err(access::denied());
    }
    Ok(())
}
