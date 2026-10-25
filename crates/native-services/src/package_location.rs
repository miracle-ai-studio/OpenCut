//! Owner-local OS dialogs. Selection never creates or replaces package content.
use opencut_api_contracts::project_exchange::{ChoosePackageLocation, PackageLocation};
use opencut_contracts::EditorError;
use opencut_editor_application::project_exchange::PackageLocationPicker;

pub struct NativePackageLocation;

impl PackageLocationPicker for NativePackageLocation {
    fn choose(&self, input: &ChoosePackageLocation) -> Result<PackageLocation, EditorError> {
        #[cfg(target_os = "macos")]
        {
            macos::choose(input)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = input;
            Err(EditorError::new(
                "CAPABILITY_UNAVAILABLE",
                "Package location dialogs are unavailable on this host",
            ))
        }
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use opencut_api_contracts::project_exchange::{PackageDialogLanguage, PackageLocationPurpose};
    use std::{
        io::Read,
        path::{Component, Path},
        process::{Child, Command, Stdio},
        sync::Mutex,
        time::{Duration, Instant},
    };

    // Dynamic text is passed only as argv, never interpolated into AppleScript.
    const SCRIPT: &str = r#"on run argv
    set purpose to item 1 of argv
    set proposedName to item 2 of argv
    set promptText to item 3 of argv
    try
        with timeout of 300 seconds
            if purpose is "save" then
                set selectedLocation to choose file name with prompt promptText default name proposedName
            else
                set selectedLocation to choose folder with prompt promptText
            end if
        end timeout
        return POSIX path of selectedLocation
    on error errorText number errorNumber
        if errorNumber is -128 then return ""
        error errorText number errorNumber
    end try
end run"#;
    static ACTIVE: Mutex<()> = Mutex::new(());
    struct DialogProcess(Child);
    impl Drop for DialogProcess {
        fn drop(&mut self) {
            // Covers timeout, read failure and unwinding; selection has no writes.
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    fn bounded(mut pipe: impl Read) -> Result<Vec<u8>, EditorError> {
        let mut bytes = Vec::new();
        pipe.by_ref()
            .take(8193)
            .read_to_end(&mut bytes)
            .map_err(|_| {
                EditorError::new(
                    "PACKAGE_DIALOG_FAILED",
                    "Cannot read the location dialog result",
                )
            })?;
        if bytes.len() > 8192 {
            return Err(EditorError::new(
                "PACKAGE_DIALOG_FAILED",
                "Location dialog result is too long",
            ));
        }
        Ok(bytes)
    }
    fn location(bytes: &[u8], save: bool) -> Result<PackageLocation, EditorError> {
        let text = std::str::from_utf8(bytes).map_err(|_| {
            EditorError::new(
                "PACKAGE_DIALOG_FAILED",
                "Location dialog returned invalid text",
            )
        })?;
        let path = text.strip_suffix('\n').unwrap_or(text);
        if path.is_empty() {
            return Ok(PackageLocation { path: None });
        }
        let selected = Path::new(path);
        if path.chars().any(char::is_control)
            || !selected.is_absolute()
            || selected
                .components()
                .any(|c| matches!(c, Component::ParentDir))
        {
            return Err(EditorError::new(
                "PACKAGE_DIALOG_FAILED",
                "Location dialog returned an invalid path",
            ));
        }
        if save && std::fs::symlink_metadata(selected).is_ok() {
            return Err(EditorError::new(
                "ALREADY_EXISTS",
                "This package location already exists. Choose a new name.",
            ));
        }
        Ok(PackageLocation {
            path: Some(path.into()),
        })
    }
    pub(super) fn choose(input: &ChoosePackageLocation) -> Result<PackageLocation, EditorError> {
        let _active = ACTIVE.try_lock().map_err(|_| {
            EditorError::new(
                "PACKAGE_DIALOG_BUSY",
                "Finish choosing the current save location first",
            )
        })?;
        let save = input.purpose == PackageLocationPurpose::Save;
        let prompt = match (save, input.language) {
            (true, PackageDialogLanguage::Zh) => "选择工程包名称和保存位置（将保存为新文件夹）",
            (false, PackageDialogLanguage::Zh) => "选择要恢复的 OpenCut 工程包文件夹",
            (true, _) => "Choose a project package name and save location (creates a new folder)",
            (false, _) => "Choose the OpenCut project package folder to restore",
        };
        let child = Command::new("/usr/bin/osascript")
            .args([
                "-e",
                SCRIPT,
                "--",
                if save { "save" } else { "open" },
                input.suggested_name.as_deref().unwrap_or("OpenCut.opencut"),
                prompt,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|_| {
                EditorError::new(
                    "CAPABILITY_UNAVAILABLE",
                    "Cannot open the package location dialog",
                )
            })?;
        let mut process = DialogProcess(child);
        let deadline = Instant::now() + Duration::from_secs(310);
        let status = loop {
            if let Some(status) = process.0.try_wait().map_err(|_| {
                EditorError::new(
                    "PACKAGE_DIALOG_FAILED",
                    "Cannot observe the location dialog",
                )
            })? {
                break status;
            }
            if Instant::now() >= deadline {
                return Err(EditorError::new(
                    "PACKAGE_DIALOG_TIMEOUT",
                    "The location dialog timed out. Choose the location again.",
                ));
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        let output = bounded(process.0.stdout.take().expect("piped output"))?;
        // Do not publish script diagnostics or unbounded paths in the main UI.
        let _ = bounded(process.0.stderr.take().expect("piped diagnostics"))?;
        if !status.success() {
            return Err(EditorError::new(
                "PACKAGE_DIALOG_FAILED",
                "The package location dialog could not be completed. Try choosing the location again.",
            ));
        }
        location(&output, save)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn cancellation_and_invalid_locations_never_become_export_targets() {
            assert!(location(b"\n", true).unwrap().path.is_none());
            for value in ["relative/path\n", "/tmp/../escape\n", "/tmp/a\nb\n"] {
                assert!(location(value.as_bytes(), true).is_err());
            }
            assert!(location(&[0xff], true).is_err());
        }
        #[test]
        fn preserves_selected_unicode_spaces_and_rejects_existing_save_targets() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("旅行剪辑.opencut");
            let text = format!("{}\n", path.display());
            assert_eq!(
                location(text.as_bytes(), true).unwrap().path.as_deref(),
                path.to_str()
            );
            std::fs::create_dir(&path).unwrap();
            assert_eq!(
                location(text.as_bytes(), true).unwrap_err().code,
                "ALREADY_EXISTS"
            );
            assert_eq!(
                location(text.as_bytes(), false).unwrap().path.as_deref(),
                path.to_str()
            );
        }
    }
}
