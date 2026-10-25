//! Host assembly adds the actual shared renderer/encoder availability checks.
use opencut_contracts::EditorError;
use opencut_editor_application::render::{
    self, FixedPreflight, PreflightRead, PreflightSource, RenderPreflight,
};
use std::path::Path;
pub(crate) fn preflight(
    app: &impl PreflightRead,
    root: &Path,
    input: render::RenderPreflightInput,
) -> Result<render::ProjectPreflightReport, EditorError> {
    render::preflight(app, &HostPreflight(root), input)
}
struct HostPreflight<'a>(&'a Path);
impl RenderPreflight for HostPreflight<'_> {
    fn inspect(
        &self,
        request: FixedPreflight,
    ) -> Result<render::ProjectPreflightReport, EditorError> {
        let editor = matches!(&request.source, PreflightSource::Editor(_));
        let input = request.input.clone();
        let report = opencut_native_services::render::NativePreflight(self.0).inspect(request)?;
        if !editor || !report.ready {
            return Ok(report);
        }
        let runtime = (|| {
            let encoders = crate::export_video::encoder_support()?;
            if !encoders.formats().contains(&input.options.format)
                || (input.options.include_audio
                    && (!encoders.audio_for(input.options.format)
                        || !crate::classic_audio_runtime::capability().available))
            {
                return Err(EditorError::new(
                    "CAPABILITY_UNAVAILABLE",
                    "The requested renderer or encoder is unavailable",
                ));
            }
            Ok::<(), EditorError>(())
        })();
        Ok(match runtime {
            Ok(()) => report,
            Err(error) => render::issue(&input, &error.code),
        })
    }
}
