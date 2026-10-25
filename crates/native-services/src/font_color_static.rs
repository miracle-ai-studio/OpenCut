//! Complete static color worker uses the existing bounded child/IO supervisor.
use opencut_contracts::EditorError;
use opencut_editor_application::{
    font_derived::{DerivedFontSource, PinnedDerivedFont},
    font_import::{FontImportCheckpoint, catalog::FontFetchPolicy},
};
use std::path::PathBuf;
pub(crate) const WRAPPER: &[u8] =
    include_bytes!("../../../scripts/font-catalog/run_color_static_v1.py");
pub(crate) const DERIVE: &[u8] =
    include_bytes!("../../../scripts/font-catalog/derive_color_static_v1.py");
pub(crate) const RUNTIME_LOCK: &[u8] =
    include_bytes!("../../../scripts/font-catalog/color-static-runtime-lock.json");
pub struct NativeConfiguredColorStaticFontSource(
    crate::font_derived::NativeConfiguredOutlineFontSource,
);
impl NativeConfiguredColorStaticFontSource {
    pub fn new(python: PathBuf, script: PathBuf, cache: Option<PathBuf>) -> Self {
        Self(
            crate::font_derived::NativeConfiguredOutlineFontSource::new_color_static(
                python, script, cache,
            ),
        )
    }
    pub fn availability(&self) -> Result<(), EditorError> {
        self.0.availability()
    }
    pub fn cached_candidate(&self, pin: &PinnedDerivedFont) -> bool {
        self.0.cached_candidate(pin)
    }
}
impl DerivedFontSource for NativeConfiguredColorStaticFontSource {
    fn obtain(
        &self,
        pin: &PinnedDerivedFont,
        policy: &FontFetchPolicy,
        checkpoint: &mut dyn FontImportCheckpoint,
    ) -> Result<Vec<u8>, EditorError> {
        self.0.obtain(pin, policy, checkpoint)
    }
}
