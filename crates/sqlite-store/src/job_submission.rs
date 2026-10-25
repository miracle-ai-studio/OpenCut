//! Same acceptance transactions, policies and immutable stored task identities.
use crate::{EditorApplication, JobRecord, jobs, sqlite_scene_transcription};
use opencut_contracts::EditorError;
use opencut_job_contracts::{
    JobStartInput, asset_analysis::SubmitAssetAnalysis,
    scene_transcription::SceneTranscriptionRequest,
};
impl opencut_editor_application::job_submission::JobSubmission for EditorApplication {
    fn asset_analysis(&mut self, input: SubmitAssetAnalysis) -> Result<JobRecord, EditorError> {
        opencut_editor_application::asset_analysis::submit(self, input)
    }
    fn scene_transcription(
        &mut self,
        input: SceneTranscriptionRequest,
    ) -> Result<JobRecord, EditorError> {
        sqlite_scene_transcription::start(self, input)
    }
    fn source_job(&mut self, input: JobStartInput) -> Result<JobRecord, EditorError> {
        crate::sqlite_access::owner(&self.connection.1)?;
        let root = self.asset_store_root().to_path_buf();
        jobs::start(self, &root, input)
    }
}
