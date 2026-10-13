//! Typed execution of a complete media/free-text frame. Font coverage arrives
//! from a native byte inspector, independently of browser raster observations.
mod protocol;
mod session;
use crate::classic_visual::ClassicVisualResources;
use opencut_contracts::EditorError;
pub use protocol::*;
pub(crate) use session::validate_layout;
pub(crate) use session::text_input;
pub use session::*;
#[cfg(test)]
pub(crate) mod tests;

pub trait ClassicTextFrameResources: ClassicVisualResources {
    fn resolve_font(
        &mut self,
        font: &TextFontResourceV2,
        handle: &str,
    ) -> Result<(String, u64), EditorError>;
}
pub trait ClassicTextFrameExecutorV2 {
    fn execute_text_frame(
        &self,
        session: &mut ClassicTextFrameSessionV2<'_>,
        resources: &mut dyn ClassicTextFrameResources,
        handles: &[crate::classic_visual::ClassicVisualResourceHandle],
        fonts: &[ClassicTextFontHandleV2],
        sink: &mut dyn crate::classic_visual::ClassicVisualRgbaSink,
    ) -> Result<ClassicTextFrameExecutionV2, EditorError>;
}
fn invalid(message: impl Into<String>) -> EditorError {
    EditorError::new("INVALID_VISUAL_PLAN", message)
}
