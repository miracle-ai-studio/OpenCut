//! Declarative presets are data; their only effect is a normal, reviewable edit plan.
mod planning;
pub use opencut_api_contracts::presets::*;
use opencut_contracts::{AssetVersion, EditorError, Operation, scene::Caption};
pub use planning::plan;
use std::collections::BTreeSet;

pub const MAX_PRESETS: usize = 32;
pub const MAX_OPERATIONS: usize = 32;
pub const MAX_PARAMETERS_BYTES: usize = 64 * 1024;
pub const MAX_SNAPSHOT_BYTES: usize = 4 * 1024 * 1024;

fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_PRESET", message)
}
fn denied() -> EditorError {
    EditorError::new(
        "PERMISSION_DENIED",
        "Project preset grant or access is unavailable",
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Authorization {
    pub grant: Grant,
    pub principal_revision: Option<String>,
}

pub trait PresetGrantStore {
    fn set_preset_grant(&mut self, input: &SetGrant) -> Result<Grant, EditorError>;
    fn preset_grants(&self, project_id: &str) -> Result<Vec<Grant>, EditorError>;
    /// Must derive rights from the host-bound caller, not input JSON.
    fn authorize_preset(
        &self,
        input: &PresetInput,
        execute: bool,
    ) -> Result<Authorization, EditorError>;
    fn revalidate_preset(
        &self,
        input: &PresetInput,
        execute: bool,
        expected: &Authorization,
    ) -> Result<(), EditorError> {
        if self.authorize_preset(input, execute)? != *expected {
            return Err(denied());
        }
        Ok(())
    }
}

pub trait PresetCatalog {
    /// Native adapter uses the original immutable preset package root and dependency lock.
    fn install(&self, input: &InstallInput) -> Result<InstalledPresetPackage, EditorError>;
    fn install_pinned(
        &self,
        input: &InstallPinnedInput,
    ) -> Result<InstalledPresetPackage, EditorError>;
    fn list(&self) -> Result<Vec<InstalledPresetPackage>, EditorError>;
    fn inspect(&self, input: &PackageInput) -> Result<InstalledPresetPackage, EditorError>;
    fn load_spec(
        &self,
        input: &PresetInput,
        expected_package_sha256: &str,
    ) -> Result<(PresetDeclaration, PresetSpec), EditorError>;
}

pub fn install(
    catalog: &impl PresetCatalog,
    input: &InstallInput,
) -> Result<InstalledPresetPackage, EditorError> {
    catalog.install(input)
}
pub fn install_pinned(
    catalog: &impl PresetCatalog,
    input: &InstallPinnedInput,
) -> Result<InstalledPresetPackage, EditorError> {
    catalog.install_pinned(input)
}
pub fn list(catalog: &impl PresetCatalog) -> Result<Vec<InstalledPresetPackage>, EditorError> {
    catalog.list()
}
pub fn inspect(
    store: &impl PresetGrantStore,
    catalog: &impl PresetCatalog,
    input: PresetInput,
) -> Result<PresetView, EditorError> {
    let ticket = store.authorize_preset(&input, false)?;
    let package = catalog.inspect(&PackageInput {
        id: input.target.id.clone(),
        version: input.target.version.clone(),
    })?;
    if package.package_sha256 != ticket.grant.package_sha256 {
        return Err(denied());
    }
    let preset = package
        .manifest
        .presets
        .iter()
        .find(|p| p.name == input.preset_name)
        .cloned()
        .ok_or_else(|| invalid("Unknown preset"))?;
    store.revalidate_preset(&input, false, &ticket)?;
    Ok(PresetView {
        package,
        grant: ticket.grant,
        preset,
    })
}
pub fn set_grant(
    store: &mut impl PresetGrantStore,
    catalog: &impl PresetCatalog,
    input: &SetGrant,
) -> Result<Grant, EditorError> {
    validate_grant(input)?;
    let package = catalog.inspect(&PackageInput {
        id: input.target.id.clone(),
        version: input.target.version.clone(),
    })?;
    if package.package_sha256 != input.package_sha256
        || input
            .preset_names
            .iter()
            .any(|name| !package.manifest.presets.iter().any(|p| &p.name == name))
    {
        return Err(invalid("Grant does not match installed package presets"));
    }
    store.set_preset_grant(input)
}

pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 80
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
pub fn valid_version(version: &str) -> bool {
    !version.is_empty()
        && version.len() <= 128
        && !version.starts_with('.')
        && !version.ends_with('.')
        && !version.contains("..")
        && version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
}
pub fn valid_package_id(id: &str) -> bool {
    id.len() <= 255 && id.split('.').all(valid_name)
}
pub fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub fn validate_grant(input: &SetGrant) -> Result<(), EditorError> {
    if input.target.project_id.is_empty()
        || input.target.project_id.len() > 1024
        || input.target.project_id.chars().any(char::is_control)
        || !valid_package_id(&input.target.id)
        || !valid_version(&input.target.version)
        || !valid_hash(&input.package_sha256)
        || input.preset_names.len() > MAX_PRESETS
        || input.preset_names.iter().any(|name| !valid_name(name))
        || (input.enabled && input.preset_names.is_empty())
        || input
            .expected_revision
            .parse::<i64>()
            .ok()
            .filter(|n| *n >= 0 && *n < i64::MAX)
            .is_none()
        || input.idempotency_key.is_empty()
        || input.idempotency_key.len() > 1024
    {
        return Err(invalid("Invalid preset grant"));
    }
    Ok(())
}

struct ExpansionFacts {
    assets: std::collections::BTreeMap<String, AssetVersion>,
    captions: std::collections::BTreeMap<(String, String), Caption>,
}
fn pinned(facts: &ExpansionFacts, asset_id: &str) -> bool {
    facts.assets.get(asset_id).is_some_and(|version| {
        version.id == format!("av_{}", version.content_hash) && valid_hash(&version.content_hash)
    })
}
fn expand(
    spec: PresetSpec,
    parameters: &PresetParameters,
    bindings: &std::collections::BTreeMap<String, String>,
    facts: &ExpansionFacts,
) -> Result<Vec<Operation>, EditorError> {
    let operations = match (spec, parameters) {
        (
            PresetSpec::CaptionStyle { style },
            PresetParameters::CaptionStyle {
                sequence_id,
                cue_ids,
            },
        ) => {
            if !bindings.is_empty() || cue_ids.is_empty() || cue_ids.len() > MAX_OPERATIONS {
                return Err(invalid("Caption style parameters are out of range"));
            }
            let unique: BTreeSet<_> = cue_ids.iter().collect();
            if unique.len() != cue_ids.len() {
                return Err(invalid("Duplicate caption cue"));
            }
            cue_ids
                .iter()
                .map(|id| {
                    let source = facts
                        .captions
                        .get(&(sequence_id.clone(), id.clone()))
                        .ok_or_else(|| invalid("Unknown caption cue"))?;
                    let cue = opencut_contracts::CaptionCue {
                        id: source.id.clone(),
                        timeline_range: source.timeline_range.clone(),
                        text: source.text.clone(),
                        source_anchor: source.source_anchor.clone(),
                        style: style.clone(),
                    };
                    Ok(Operation::UpdateCaption {
                        sequence_id: sequence_id.clone(),
                        cue,
                    })
                })
                .collect::<Result<Vec<_>, EditorError>>()?
        }
        (
            PresetSpec::EditTemplate {
                mut operations,
                clip_bindings,
            },
            PresetParameters::EditTemplate {},
        ) => {
            if operations.is_empty()
                || operations.len() > MAX_OPERATIONS
                || bindings.len() > MAX_OPERATIONS
            {
                return Err(invalid("Template operation limit exceeded"));
            }
            let mut used = BTreeSet::new();
            for operation in &mut operations {
                match operation {
                    Operation::AddTrack { track, .. } if track.clips.is_empty() => {}
                    Operation::InsertCaption { cue, .. } => {
                        if let Some(anchor) = &cue.source_anchor
                            && !pinned(facts, &anchor.asset_id)
                        {
                            return Err(invalid("Caption source is not pinned"));
                        }
                    }
                    Operation::InsertClip { clip, .. } => {
                        if clip.asset_id.is_some() {
                            return Err(invalid("Template clip may not contain an asset id"));
                        }
                        let key = clip_bindings
                            .get(&clip.id)
                            .ok_or_else(|| invalid("Clip binding is missing"))?;
                        let asset_id = bindings
                            .get(key)
                            .ok_or_else(|| invalid("Asset binding is missing"))?;
                        if !pinned(facts, asset_id) {
                            return Err(invalid("Asset is not pinned in fixed revision"));
                        }
                        clip.asset_id = Some(asset_id.clone());
                        used.insert(key.clone());
                    }
                    _ => {
                        return Err(invalid(
                            "Template operation is outside the declarative allowlist",
                        ));
                    }
                }
            }
            if clip_bindings.values().any(|key| !used.contains(key))
                || bindings.keys().any(|key| !used.contains(key))
            {
                return Err(invalid("Unexpected asset binding"));
            }
            operations
        }
        _ => return Err(invalid("Preset kind and parameters differ")),
    };
    Ok(operations)
}
