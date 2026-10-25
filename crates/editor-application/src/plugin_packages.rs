//! A single use-case surface over the original WASM and data-only catalogs.
//! No installation path, runtime, alternate protocol or project grant is inferred.
use crate::{plugins, presets};
pub use opencut_api_contracts::plugin_packages::*;
use opencut_contracts::EditorError;

/// Native input observation only. Each selected installer must still validate
/// the entire source and exact pin under its original publication/version fence.
pub trait PluginPackageSource {
    fn kind(&self, root: &str) -> Result<PluginPackageKind, EditorError>;
}
fn wasm(value: plugins::InstalledWasmPlugin) -> InstalledPluginPackage {
    InstalledPluginPackage::Wasm {
        manifest: value.manifest,
        package_sha256: value.package_sha256,
        granted_permissions: value.granted_permissions,
    }
}
fn preset(value: presets::InstalledPresetPackage) -> InstalledPluginPackage {
    InstalledPluginPackage::Presets {
        manifest: value.manifest,
        package_sha256: value.package_sha256,
    }
}
pub fn list(
    wasm_catalog: &impl plugins::PluginCatalog,
    preset_catalog: &impl presets::PresetCatalog,
) -> Result<Vec<InstalledPluginPackage>, EditorError> {
    let mut values: Vec<_> = plugins::list(wasm_catalog)?.into_iter().map(wasm).collect();
    values.extend(presets::list(preset_catalog)?.into_iter().map(preset));
    let identity = |value: &InstalledPluginPackage| match value {
        InstalledPluginPackage::Wasm { manifest, .. } => (
            manifest.id.clone(),
            manifest.version.clone(),
            PluginPackageKind::Wasm,
        ),
        InstalledPluginPackage::Presets { manifest, .. } => (
            manifest.id.clone(),
            manifest.version.clone(),
            PluginPackageKind::Presets,
        ),
    };
    values.sort_by_key(identity);
    Ok(values)
}
pub fn inspect(
    wasm_catalog: &impl plugins::PluginCatalog,
    preset_catalog: &impl presets::PresetCatalog,
    input: &InspectPluginPackage,
) -> Result<InstalledPluginPackage, EditorError> {
    match input.kind {
        PluginPackageKind::Wasm => plugins::inspect(
            wasm_catalog,
            plugins::PluginInput {
                id: input.id.clone(),
                version: input.version.clone(),
            },
        )
        .map(wasm),
        PluginPackageKind::Presets => presets::PresetCatalog::inspect(
            preset_catalog,
            &presets::PackageInput {
                id: input.id.clone(),
                version: input.version.clone(),
            },
        )
        .map(preset),
    }
}
pub fn install(
    wasm_catalog: &impl plugins::PluginCatalog,
    preset_catalog: &impl presets::PresetCatalog,
    source: &impl PluginPackageSource,
    input: &InstallPluginPackage,
) -> Result<InstalledPluginPackage, EditorError> {
    if input.package_root.is_empty()
        || input.package_root.len() > 16384
        || input.package_root.chars().any(char::is_control)
        || input.expected_package_sha256.as_ref().is_some_and(|hash| {
            hash.len() != 64
                || !hash
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
    {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Invalid plugin package input or content pin",
        ));
    }
    match source.kind(&input.package_root)? {
        PluginPackageKind::Wasm => {
            let permissions: Vec<_> = input.granted_permissions.iter().cloned().collect();
            if let Some(hash) = &input.expected_package_sha256 {
                plugins::install_pinned(
                    wasm_catalog,
                    PinnedPluginPackage {
                        package_root: input.package_root.clone(),
                        granted_permissions: permissions,
                        expected_package_sha256: hash.clone(),
                    },
                )
                .map(wasm)
            } else {
                plugins::install(
                    wasm_catalog,
                    plugins::InstallInput {
                        package_root: input.package_root.clone(),
                        granted_permissions: permissions,
                    },
                )
                .map(wasm)
            }
        }
        PluginPackageKind::Presets => {
            if !input.granted_permissions.is_empty() {
                return Err(EditorError::new(
                    "INVALID_ARGUMENT",
                    "Declarative presets do not request runtime permissions",
                ));
            }
            if let Some(hash) = &input.expected_package_sha256 {
                presets::install_pinned(
                    preset_catalog,
                    &presets::InstallPinnedInput {
                        package_root: input.package_root.clone(),
                        expected_package_sha256: hash.clone(),
                    },
                )
                .map(preset)
            } else {
                presets::install(
                    preset_catalog,
                    &presets::InstallInput {
                        package_root: input.package_root.clone(),
                    },
                )
                .map(preset)
            }
        }
    }
}
