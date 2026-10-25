//! Renamed V2 wrapper shares bounded native IO/child supervision with V1 while
//! binding its own worker, algorithm and runtime identity. No new dependencies.
use opencut_contracts::EditorError;
use opencut_editor_application::{
    font_derived::{DerivedFontSource, PinnedDerivedFont},
    font_import::{FontImportCheckpoint, catalog::FontFetchPolicy},
};
use std::path::PathBuf;
pub(crate) const WRAPPER: &[u8] =
    include_bytes!("../../../../scripts/font-catalog/run_derived_v2.py");
pub(crate) const DERIVE: &[u8] =
    include_bytes!("../../../../scripts/font-catalog/derive_renamed_v2.py");
pub(crate) const RUNTIME_LOCK: &[u8] =
    include_bytes!("../../../../scripts/font-catalog/derived-v2-runtime-lock.json");
pub struct NativeConfiguredNamedFontSource(crate::font_derived::NativeConfiguredOutlineFontSource);
impl NativeConfiguredNamedFontSource {
    pub fn new(python: PathBuf, script: PathBuf, cache: Option<PathBuf>) -> Self {
        Self(
            crate::font_derived::NativeConfiguredOutlineFontSource::new_renamed(
                python, script, cache,
            ),
        )
    }
    pub fn availability(&self) -> Result<(), EditorError> {
        self.0.availability()
    }
    pub fn cached(&self, pin: &PinnedDerivedFont) -> bool {
        self.0.cached(pin)
    }
    pub fn cached_candidate(&self, pin: &PinnedDerivedFont) -> bool {
        self.0.cached_candidate(pin)
    }
}
impl DerivedFontSource for NativeConfiguredNamedFontSource {
    fn obtain(
        &self,
        source: &PinnedDerivedFont,
        policy: &FontFetchPolicy,
        checkpoint: &mut dyn FontImportCheckpoint,
    ) -> Result<Vec<u8>, EditorError> {
        self.0.obtain(source, policy, checkpoint)
    }
}
