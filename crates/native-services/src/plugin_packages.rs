//! Source classification only. The original installers own all byte validation,
//! immutable identity, locks, grants, budget, staging and publication behavior.
use opencut_contracts::EditorError;
use opencut_editor_application::plugin_packages::{PluginPackageKind, PluginPackageSource};
use std::path::Path;

pub struct NativePluginPackageSource;
impl PluginPackageSource for NativePluginPackageSource {
    fn kind(&self, root: &str) -> Result<PluginPackageKind, EditorError> {
        let invalid = || {
            EditorError::new(
                "INVALID_PLUGIN",
                "Package must contain exactly one supported regular manifest",
            )
        };
        let path = Path::new(root);
        if !path.is_absolute() {
            return Err(EditorError::new(
                "INVALID_ARGUMENT",
                "Plugin installation requires an absolute owner-local source",
            ));
        }
        let path = path.canonicalize().map_err(|_| invalid())?;
        if !std::fs::symlink_metadata(&path)
            .map_err(|_| invalid())?
            .is_dir()
        {
            return Err(invalid());
        }
        let manifest = |name: &str| -> Result<bool, EditorError> {
            match std::fs::symlink_metadata(path.join(name)) {
                Ok(meta) if meta.file_type().is_file() => Ok(true),
                Ok(_) => Err(invalid()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
                Err(_) => Err(invalid()),
            }
        };
        match (manifest("manifest.json")?, manifest("manifest_v2.json")?) {
            (true, false) => Ok(PluginPackageKind::Wasm),
            (false, true) => Ok(PluginPackageKind::Presets),
            _ => Err(invalid()),
        }
    }
}
