//! Trusted visual runtime configuration, separate from all public frame inputs.
use crate::browser_runtime::{absolute, dependency, executable, metadata, package_file};
use opencut_contracts::EditorError;
use opencut_job_contracts::project_v4_frame::V4FramePngCapabilityV1;
use opencut_job_execution::project_v4_frame::LocalVisualExecutionPolicyV1;
use opencut_native_services::classic_visual::ClassicVisualProcessConfig;
use std::{
    env,
    path::{Path, PathBuf},
    time::Duration,
};

fn unavailable() -> EditorError {
    EditorError::new("CAPABILITY_UNAVAILABLE", "VISUAL_RUNTIME_UNAVAILABLE")
}
fn paths() -> Result<(PathBuf, PathBuf, PathBuf), EditorError> {
    let package = match env::var_os("OPENCUT_CLASSIC_VISUAL_PACKAGE") {
        Some(path) => PathBuf::from(path),
        None => env::current_exe()
            .ok()
            .as_deref()
            .and_then(Path::parent)
            .map(|parent| parent.join("libexec/classic-visual-renderer"))
            .filter(|path| path.is_dir())
            .ok_or_else(|| {
                EditorError::new("CAPABILITY_UNAVAILABLE", "VISUAL_RUNTIME_NOT_CONFIGURED")
            })?,
    };
    let package = absolute(&package).map_err(|_| unavailable())?;
    let chromium = env::var_os("OPENCUT_CLASSIC_VISUAL_CHROMIUM").ok_or_else(|| {
        EditorError::new("CAPABILITY_UNAVAILABLE", "VISUAL_RUNTIME_NOT_CONFIGURED")
    })?;
    let chromium = executable(Path::new(&chromium)).map_err(|_| unavailable())?;
    let node = match env::var_os("OPENCUT_CLASSIC_VISUAL_NODE") {
        Some(path) => executable(Path::new(&path)).map_err(|_| unavailable())?,
        None => env::var_os("PATH")
            .as_deref()
            .into_iter()
            .flat_map(env::split_paths)
            .filter(|path| path.is_absolute())
            .find_map(|path| executable(&path.join("node")).ok())
            .ok_or_else(unavailable)?,
    };
    let manifest = metadata(&package.join("package.json")).map_err(|_| unavailable())?;
    if manifest["name"] != "@opencut/classic-visual-renderer"
        || manifest["version"] != "0.1.0"
        || manifest["type"] != "module"
        || manifest["dependencies"]["playwright-core"] != "1.63.0"
        || manifest["dependencies"]["mediabunny"] != "1.41.0"
        || manifest["dependencies"]["@opencut/classic-audio-renderer"]
            != "file:../classic-audio-renderer"
    {
        return Err(unavailable());
    }
    dependency(&package, "playwright-core", "1.63.0", "index.mjs").map_err(|_| unavailable())?;
    dependency(
        &package,
        "mediabunny",
        "1.41.0",
        "dist/bundles/mediabunny.mjs",
    )
    .map_err(|_| unavailable())?;
    dependency(
        &package,
        "@opencut/classic-audio-renderer",
        "0.1.0",
        "host/shared.mjs",
    )
    .map_err(|_| unavailable())?;
    let shared = absolute(&package.join("node_modules/@opencut/classic-audio-renderer"))
        .map_err(|_| unavailable())?;
    let shared_manifest = metadata(&shared.join("package.json")).map_err(|_| unavailable())?;
    if shared_manifest["exports"]["./internal-host-runtime"] != "./host/shared.mjs" {
        return Err(unavailable());
    }
    for file in [
        "host/runner.mjs",
        "host/browser.mjs",
        "host/modules.mjs",
        "host/limits.mjs",
        "host/protocol.mjs",
        "host/page.mjs",
        "src/validation.mjs",
    ] {
        package_file(&shared, file).map_err(|_| unavailable())?;
    }
    for file in [
        "host/browser.mjs",
        "host/modules.mjs",
        "host/page.mjs",
        "host/protocol.mjs",
        "src/index.mjs",
        "src/decoder.mjs",
        "src/image-format.mjs",
        "src/gpu.mjs",
        "src/validation.mjs",
        "vendor/classic-v31/layer.wgsl",
        "vendor/classic-v31/blend.wgsl",
        "vendor/classic-v31/fullscreen.wgsl",
    ] {
        package_file(&package, file).map_err(|_| unavailable())?;
    }
    let runner = package_file(&package, "host/runner.mjs").map_err(|_| unavailable())?;
    Ok((node, chromium, runner))
}
pub(crate) fn configured(
    policy: &LocalVisualExecutionPolicyV1,
) -> Result<ClassicVisualProcessConfig, EditorError> {
    opencut_editor_application::project_v4_frame_jobs::validate_policy(policy)?;
    let (node, chromium, runner) = paths()?;
    Ok(ClassicVisualProcessConfig {
        node,
        chromium,
        runner,
        deadline: Duration::from_millis(policy.deadline_ms),
        policy: policy.clone(),
        strict_cgroup: None,
    })
}
pub(crate) fn capability() -> V4FramePngCapabilityV1 {
    let result = paths();
    V4FramePngCapabilityV1 {
        schema_version: 1,
        available: result.is_ok(),
        submit_tool: "opencut_job_v4_frame_png".into(),
        unavailable_reason: result.err().map(|error| error.message),
    }
}

/// The V2 capability requires its additional source modules. V1 configuration
/// remains valid for older installed runtimes that lack text rendering.
pub(crate) fn text_capability() -> opencut_job_contracts::project_v4_text_frame::V4TextFramePngCapabilityV2 {
    let result = paths().and_then(|(_, _, runner)| check_text_modules(&runner));
    opencut_job_contracts::project_v4_text_frame::V4TextFramePngCapabilityV2 {
        schema_version: 2,
        available: result.is_ok(),
        submit_tool: "opencut_job_v4_text_frame_png".into(),
        unavailable_reason: result.err().map(|error| error.message),
    }
}

fn check_text_modules(runner: &Path) -> Result<(), EditorError> {
    let package = runner.parent().and_then(Path::parent).ok_or_else(unavailable)?;
    package_file(package, "src/text-frame.mjs").map_err(|_| unavailable())?;
    package_file(package, "src/text-layout.mjs").map_err(|_| unavailable())?;
    Ok(())
}
pub(crate) fn configured_text(policy: &LocalVisualExecutionPolicyV1) -> Result<ClassicVisualProcessConfig, EditorError> {
    let config = configured(policy)?;
    check_text_modules(&config.runner)?;
    Ok(config)
}
