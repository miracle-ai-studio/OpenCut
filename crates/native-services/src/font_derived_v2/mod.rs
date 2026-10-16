//! Renamed V2 wrapper shares bounded native IO/child supervision with V1 while
//! binding its own worker, algorithm and runtime identity. No new dependencies.
use opencut_contracts::EditorError;
use opencut_editor_application::{
    font_derived_v1::{DerivedFontSourceV1, PinnedDerivedFontV1},
    font_import_v1::{FontImportCheckpointV1, catalog::FontFetchPolicyV1},
};
use std::path::PathBuf;
pub(crate) const WRAPPER: &[u8] =
    include_bytes!("../../../../scripts/font-catalog/run_derived_v2.py");
pub(crate) const DERIVE: &[u8] =
    include_bytes!("../../../../scripts/font-catalog/derive_renamed_v2.py");
pub(crate) const RUNTIME_LOCK: &[u8] =
    include_bytes!("../../../../scripts/font-catalog/derived-v2-runtime-lock.json");
pub struct NativeConfiguredDerivedFontSourceV2(
    crate::font_derived_v1::NativeConfiguredDerivedFontSourceV1,
);
impl NativeConfiguredDerivedFontSourceV2 {
    pub fn new(python: PathBuf, script: PathBuf, cache: Option<PathBuf>) -> Self {
        Self(
            crate::font_derived_v1::NativeConfiguredDerivedFontSourceV1::new_renamed_v2(
                python, script, cache,
            ),
        )
    }
    pub fn availability(&self) -> Result<(), EditorError> {
        self.0.availability()
    }
    pub fn cached(&self, pin: &PinnedDerivedFontV1) -> bool {
        self.0.cached(pin)
    }
    pub fn cached_candidate(&self, pin: &PinnedDerivedFontV1) -> bool {
        self.0.cached_candidate(pin)
    }
}
impl DerivedFontSourceV1 for NativeConfiguredDerivedFontSourceV2 {
    fn obtain(
        &self,
        source: &PinnedDerivedFontV1,
        policy: &FontFetchPolicyV1,
        checkpoint: &mut dyn FontImportCheckpointV1,
    ) -> Result<Vec<u8>, EditorError> {
        self.0.obtain(source, policy, checkpoint)
    }
}
