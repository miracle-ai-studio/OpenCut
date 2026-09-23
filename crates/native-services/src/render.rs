use opencut_contracts::EditorError;
use opencut_editor_application::{
    access::ProjectExecutionRead,
    render::{FixedPreflight, RenderPreflight},
};
use opencut_project_render::{
    PreflightIssue, ProjectPreflightReport, ProjectRenderAdapter, ResolvedProjectPreflightRequest,
};
use std::path::Path;

pub use opencut_api_contracts::render::RenderPreflightInput;

pub struct NativePreflight<'a>(pub &'a Path);
pub fn preflight(
    app: &impl ProjectExecutionRead,
    asset_store_root: &Path,
    input: RenderPreflightInput,
) -> Result<ProjectPreflightReport, EditorError> {
    opencut_editor_application::render::preflight(app, &NativePreflight(asset_store_root), input)
}
impl RenderPreflight for NativePreflight<'_> {
    fn inspect(&self, input: FixedPreflight) -> Result<ProjectPreflightReport, EditorError> {
        let revision = input.snapshot.revision.clone();
        let snapshot = input.snapshot;
        let asset_store_root = self.0;
        let mut store = None;
        let resolved =
            match ProjectRenderAdapter::resolve_assets(snapshot, &input.sequence_id, |asset| {
                let store = store.get_or_insert_with(|| {
                    opencut_asset_store::AssetStore::read_only(asset_store_root)
                });
                match store {
                    Ok(store) => store
                        .resolve_asset(asset)
                        .map(|resolved| std::path::PathBuf::from(resolved.uri)),
                    Err(error) => Err(error.clone()),
                }
            }) {
                Ok(resolved) => resolved,
                Err(error) => {
                    let code = match error.code {
                        "NOT_FOUND" => "SEQUENCE_NOT_FOUND",
                        code => code,
                    };
                    return Ok(ProjectPreflightReport {
                        revision,
                        sequence_id: input.sequence_id,
                        ready: false,
                        issues: vec![PreflightIssue {
                            code: code.into(),
                            message: error.message,
                            asset_id: None,
                            clip_id: None,
                            cue_id: None,
                        }],
                    });
                }
            };
        Ok(ProjectRenderAdapter::preflight_resolved(
            &ResolvedProjectPreflightRequest {
                resolved: &resolved,
                container: input.container,
                video_codec: input.video_codec,
            },
        ))
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

        let report = preflight(
            &app,
            &directory.path().join("asset-store"),
            RenderPreflightInput {
                project_id: "project-1".into(),
                revision: snapshot.revision.clone(),
                sequence_id: "missing".into(),
                container: Container::Mp4,
                video_codec: VideoCodec::H264,
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
