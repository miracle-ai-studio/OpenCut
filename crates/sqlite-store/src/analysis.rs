use crate::EditorApplication;
use opencut_contracts::EditorError;
use opencut_editor_application::{JobRecord, analysis};
use opencut_job_contracts::{ShotsJobInput, SilenceJobInput, WaveformJobInput};
use std::path::Path;

pub fn start_silence(
    app: &mut EditorApplication,
    asset_store_root: &Path,
    input: SilenceJobInput,
) -> Result<JobRecord, EditorError> {
    analysis::submit_silence(app, &super::jobs::native_assets(asset_store_root), input)
}
pub fn start_shots(
    app: &mut EditorApplication,
    asset_store_root: &Path,
    input: ShotsJobInput,
) -> Result<JobRecord, EditorError> {
    analysis::submit_shots(app, &super::jobs::native_assets(asset_store_root), input)
}
pub fn start_waveform(
    app: &mut EditorApplication,
    asset_store_root: &Path,
    input: WaveformJobInput,
) -> Result<JobRecord, EditorError> {
    analysis::submit_waveform(app, &super::jobs::native_assets(asset_store_root), input)
}
