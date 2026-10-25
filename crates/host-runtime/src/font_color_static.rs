//! Host configuration for complete static color recipes; shared import fences.
use super::*;
use opencut_contracts::font_derived::*;
use opencut_editor_application::{
    font_color_static as app,
    font_derived::{DerivedFontSource, PinnedDerivedFont},
    font_import::{FontImportCheckpoint, catalog::FontFetchPolicy},
};
use opencut_native_services::font_color_static::NativeConfiguredColorStaticFontSource;

pub(super) fn configured() -> Result<NativeConfiguredColorStaticFontSource, EditorError> {
    let (python, script, cache) = super::font_named::configured_paths(
        "OPENCUT_FONT_COLOR_STATIC_SCRIPT",
        "run_color_static_v1.py",
    )?;
    Ok(NativeConfiguredColorStaticFontSource::new(
        python, script, cache,
    ))
}
struct HostSource;
impl DerivedFontSource for HostSource {
    fn obtain(
        &self,
        pin: &PinnedDerivedFont,
        policy: &FontFetchPolicy,
        checkpoint: &mut dyn FontImportCheckpoint,
    ) -> Result<Vec<u8>, EditorError> {
        checkpoint.check()?;
        configured()?.obtain(pin, policy, checkpoint)
    }
}
pub(super) fn import(
    host: &mut Automation,
    request: &ImportFont,
) -> Result<opencut_contracts::TransactionResult, EditorError> {
    app::validate_import_request(request)?;
    super::font_named::import_with_source(host, request, &HostSource)
}
