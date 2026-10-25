//! Existing historical media graphs and complete editor export admission share one port.
use opencut_api_contracts::render::ProjectPreflightReport;
pub use opencut_api_contracts::render::RenderPreflightInput;
use opencut_contracts::EditorError;
use opencut_editor_application::render::{FixedPreflight, PreflightSource, RenderPreflight, issue};
use std::path::Path;
pub struct NativePreflight<'a>(pub &'a Path);
impl RenderPreflight for NativePreflight<'_> {
    fn inspect(&self, request: FixedPreflight) -> Result<ProjectPreflightReport, EditorError> {
        match request.source {
            PreflightSource::Editor(authorized) => editor(self.0, authorized, request.input),
            _ => Ok(issue(&request.input, "PROJECT_PROMOTION_REQUIRED")),
        }
    }
}

fn editor(
    root: &Path,
    authorized: opencut_editor_application::media_video_jobs::AuthorizedVideoSnapshot,
    input: RenderPreflightInput,
) -> Result<ProjectPreflightReport, EditorError> {
    use opencut_editor_application::{export_video as export, media_video_jobs as video};
    let result = (|| {
        let profile = export::select_profile(&authorized.snapshot, &input.sequence_id)?;
        let prepared = video::prepare_with_frame_window(
            &authorized.snapshot,
            &input.sequence_id,
            &authorized.processing_policy,
            export::frame_profile(profile),
            input.options.include_audio,
            input.range.as_ref(),
        )?;
        let store = opencut_asset_store::AssetStore::read_only(root)?;
        for id in &prepared.asset_ids {
            let asset = authorized
                .snapshot
                .project
                .assets
                .iter()
                .find(|a| &a.id == id)
                .ok_or_else(|| {
                    EditorError::new("ASSET_LOCK_INVALID", "Fixed export asset is missing")
                })?;
            store.open_version_verified(&asset.version)?;
        }
        for id in &prepared.font_ids {
            let font = authorized
                .snapshot
                .project
                .fonts
                .iter()
                .find(|f| &f.id == id)
                .ok_or_else(|| {
                    EditorError::new("FONT_LOCK_INVALID", "Fixed export font is missing")
                })?;
            store.open_version_verified(&font.content)?;
        }
        Ok::<(), EditorError>(())
    })();
    match result {
        Ok(()) => Ok(ProjectPreflightReport {
            revision: input.revision,
            sequence_id: input.sequence_id,
            ready: true,
            issues: vec![],
        }),
        Err(error) => Ok(issue(&input, &error.code)),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use opencut_contracts::{CreateProject, FrameRate};
    use opencut_media::{Container, VideoCodec};
    use opencut_sqlite_store::EditorApplication;

    #[test]
    fn preflight_uses_the_requested_immutable_revision_and_returns_typed_issues() {
        let directory = tempfile::tempdir().unwrap();
        let mut app = EditorApplication::open(directory.path().join("workspace.sqlite")).unwrap();
        let snapshot = app
            .create_project(
                CreateProject {
                    id: "project-1".into(),
                    name: "Preflight".into(),
                    sequence_id: "main".into(),
                    width: 1920,
                    height: 1080,
                    fps: FrameRate {
                        numerator: 30,
                        denominator: 1,
                    },
                },
                "create",
            )
            .unwrap();

        let report = opencut_editor_application::render::preflight(
            &app,
            &NativePreflight(&directory.path().join("asset-store")),
            RenderPreflightInput {
                project_id: "project-1".into(),
                revision: snapshot.revision.clone(),
                sequence_id: "missing".into(),
                options: Default::default(),
                range: None,
            },
        )
        .unwrap();

        assert_eq!(report.revision, snapshot.revision);
        assert_eq!(report.sequence_id, "missing");
        assert!(!report.ready);
        assert_eq!(report.issues.len(), 1);
        assert_eq!(report.issues[0].code, "SEQUENCE_NOT_FOUND");
        assert_eq!(report.issues[0].asset_id, None);
    }
}
