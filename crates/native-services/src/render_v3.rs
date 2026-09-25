use opencut_contracts::EditorError;
use opencut_editor_application::render_v3::{
    FixedPreflightV3, ProjectExecutionReadV3, RenderPreflightV3,
};
use opencut_project_render::{
    PreflightIssue, ProjectPreflightReport,
    project_v3::{ProjectRenderAdapterV3, ResolvedProjectPreflightRequestV3},
};
use std::path::Path;

pub use opencut_api_contracts::render::RenderPreflightInput;

pub struct NativePreflightV3<'a>(pub &'a Path);
pub fn preflight(
    app: &impl ProjectExecutionReadV3,
    asset_store_root: &Path,
    input: RenderPreflightInput,
) -> Result<ProjectPreflightReport, EditorError> {
    opencut_editor_application::render_v3::preflight(
        app,
        &NativePreflightV3(asset_store_root),
        input,
    )
}
impl RenderPreflightV3 for NativePreflightV3<'_> {
    fn inspect_v3(&self, request: FixedPreflightV3) -> Result<ProjectPreflightReport, EditorError> {
        let revision = request.snapshot.revision.clone();
        let snapshot = request.snapshot;
        let input = request.input;
        let asset_store_root = self.0;
        let mut store = None;
        let resolved =
            match ProjectRenderAdapterV3::resolve_assets(snapshot, &input.sequence_id, |asset| {
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
        Ok(ProjectRenderAdapterV3::preflight_resolved(
            &ResolvedProjectPreflightRequestV3 {
                resolved: &resolved,
                container: input.container,
                video_codec: input.video_codec,
            },
        ))
    }
}
