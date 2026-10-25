//! Caption use cases reuse the V4 commit kernel and its original request receipts.
use super::*;
use opencut_api_contracts::{subtitle_exchange::SubtitleExportReceipt, subtitles::*};
use opencut_contracts::{scene::EditorSnapshot, scene_ops::ApplySceneProject};
use opencut_editor_application::{
    audit::ExecutionContext,
    scene::{self, SceneCommitIntent, SceneRead, SceneStore},
    scene_transcription::PreparedSceneTranscription,
};
use opencut_job_contracts::scene_transcription::*;

pub(super) fn require_generation_source(
    connection: &Connection,
    context: &ExecutionContext,
    input: &GenerateCaptions,
) -> Result<SceneTranscriptionResult, EditorError> {
    require_transcript_source(
        connection,
        context,
        &input.project_id,
        &input.expected_revision,
        &input.sequence_id,
        &input.analysis_job_id,
    )
}
fn require_transcript_source(
    connection: &Connection,
    context: &ExecutionContext,
    project_id: &str,
    revision: &str,
    sequence_id: &str,
    analysis_job_id: &str,
) -> Result<SceneTranscriptionResult, EditorError> {
    sqlite_access::require_job(connection, context, analysis_job_id, ProjectAction::Read)?;
    let job = load_job(connection, analysis_job_id)?;
    if job.kind != SCENE_TRANSCRIPTION_KIND || job.state != JobState::Succeeded {
        return Err(EditorError::new(
            "ANALYSIS_NOT_READY",
            "A successful V4 whole-scene transcription is required",
        ));
    }
    let prepared: PreparedSceneTranscription = serde_json::from_value(job.input.clone())
        .map_err(|_| EditorError::new("INVALID_STORED_RESULT", "Transcript source is invalid"))?;
    // A formerly authorized job cannot be used after the original caller loses
    // access. Completion validation also binds its exact snapshot/mix evidence.
    sqlite_scene_transcription::require_completion(connection, &job)?;
    if prepared.audio.snapshot.project.id != project_id
        || prepared.audio.snapshot.revision != revision
        || prepared.audio.sequence_id != sequence_id
    {
        return Err(EditorError::new(
            "REVISION_CONFLICT",
            "Caption generation requires the original transcribed V4 revision",
        ));
    }
    opencut_editor_application::scene_transcription::validate_stored_result(
        &prepared,
        job.result.ok_or_else(|| {
            EditorError::new("INVALID_STORED_RESULT", "Transcript is unavailable")
        })?,
    )
}
/// Called by the existing V4 commit kernel under its publication transaction.
/// Re-plan from the fixed immutable source to reject a tampered prepared edit.
pub(super) fn require_caption_proposal(
    connection: &Connection,
    context: &ExecutionContext,
    intent: SceneCommitIntent<'_>,
    proposed: &EditorSnapshot,
) -> Result<(), EditorError> {
    let request = match intent {
        SceneCommitIntent::CaptionGenerate(input) => {
            let transcript = require_generation_source(connection, context, input)?;
            let fixed =
                sqlite_scene_jobs::fixed(connection, &input.project_id, &input.expected_revision)?;
            opencut_editor_application::captions::generate(&fixed, input, &transcript)?
        }
        SceneCommitIntent::CaptionImport(input) => {
            let fixed =
                sqlite_scene_jobs::fixed(connection, &input.project_id, &input.expected_revision)?;
            opencut_editor_application::captions::import(&fixed, input)?
        }
        _ => return Ok(()),
    };
    let fixed =
        sqlite_scene_jobs::fixed(connection, &request.project_id, &request.expected_revision)?;
    let expected = scene::plan(&fixed, &request)?;
    if proposed.project != expected {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Prepared captions differ from the original request",
        ));
    }
    Ok(())
}
fn cue_count(request: &ApplySceneProject, source: &EditorSnapshot, sequence_id: &str) -> usize {
    let opencut_contracts::scene_ops::ProjectOperation::SequencePut { sequence } =
        &request.operations[0]
    else {
        return 0;
    };
    let before = source
        .project
        .sequences
        .iter()
        .find(|s| s.id == sequence_id)
        .map(|s| s.captions.len())
        .unwrap_or(0);
    sequence.captions.len() - before
}
impl EditorApplication {
    /// Read-only authoring uses the same successful-job, original-caller and
    /// fixed-mix checks as the frozen caption mutation. The eventual text edit
    /// uses the existing public apply transaction and its own recovery receipt.
    pub fn subtitle_text_transcript(
        &self,
        input: &opencut_contracts::subtitle_text::SubtitleTextPlanRequest,
    ) -> Result<Option<SceneTranscriptionResult>, EditorError> {
        use opencut_contracts::subtitle_text::SubtitleTextSource;
        let SubtitleTextSource::Transcript {
            analysis_job_id, ..
        } = &input.source
        else {
            return Ok(None);
        };
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        require_transcript_source(
            &tx,
            &self.connection.1,
            &input.project_id,
            &input.expected_revision,
            &input.sequence_id,
            analysis_job_id,
        )
        .map(Some)
    }
    pub fn generate_scene_captions(
        &mut self,
        input: &GenerateCaptions,
    ) -> Result<CaptionReceipt, EditorError> {
        let intent = SceneCommitIntent::CaptionGenerate(input);
        require_idempotency_key(&input.idempotency_key)?;
        self.authorize_project(intent)?;
        let transcript = {
            let tx = self
                .connection
                .unchecked_transaction()
                .map_err(storage_error)?;
            require_generation_source(&tx, &self.connection.1, input)?
        };
        let fixed = sqlite_scene_jobs::fixed(
            &self.connection,
            &input.project_id,
            &input.expected_revision,
        )?;
        let request = opencut_editor_application::captions::generate(&fixed, input, &transcript)?;
        let count = cue_count(&request, &fixed, &input.sequence_id);
        let source = sqlite_scene::load_source(
            &self.connection,
            &input.project_id,
            Some(&input.expected_revision),
        )?;
        let proposed = scene::plan(&fixed, &request)?;
        let prepared = scene::prepare_caption_commit(&source, proposed, intent)?;
        let transaction = self.commit_scene_atomic(intent, prepared)?;
        Ok(CaptionReceipt{transaction,cue_count:count,warnings:vec!["Caption timing is estimated proportionally within locally inferred speech segments; word timestamps are not measured.".into()]})
    }
    pub fn import_scene_subtitles(
        &mut self,
        input: &SubtitleImport,
    ) -> Result<CaptionReceipt, EditorError> {
        let intent = SceneCommitIntent::CaptionImport(input);
        require_idempotency_key(&input.idempotency_key)?;
        self.authorize_project(intent)?;
        let fixed = sqlite_scene_jobs::fixed(
            &self.connection,
            &input.project_id,
            &input.expected_revision,
        )?;
        let request = opencut_editor_application::captions::import(&fixed, input)?;
        let count = cue_count(&request, &fixed, &input.sequence_id);
        let source = sqlite_scene::load_source(
            &self.connection,
            &input.project_id,
            Some(&input.expected_revision),
        )?;
        let proposed = scene::plan(&fixed, &request)?;
        let prepared = scene::prepare_caption_commit(&source, proposed, intent)?;
        let transaction = self.commit_scene_atomic(intent, prepared)?;
        Ok(CaptionReceipt {
            transaction,
            cue_count: count,
            warnings: if input.format == SubtitleFormat::Ass {
                vec!["ASS text and timing imported; styles are replaced by the explicitly selected pinned V4 style. ASS does not supply trusted font bytes.".into()]
            } else {
                vec![]
            },
        })
    }
    pub fn export_subtitles(
        &self,
        input: &SubtitleExport,
    ) -> Result<SubtitleExportReceipt, EditorError> {
        opencut_editor_application::subtitles::export(self, input)
    }
}

#[cfg(test)]
mod tests;

/// The complete public caption receipt is persisted in the same V4 commit lock.
pub(super) fn caption_receipt(
    connection: &Connection,
    intent: SceneCommitIntent<'_>,
    transaction: &TransactionResult,
) -> Result<CaptionReceipt, EditorError> {
    let (scene,warnings)=match intent{
        SceneCommitIntent::CaptionGenerate(input)=>(&input.sequence_id,vec!["Caption timing is estimated proportionally within locally inferred speech segments; word timestamps are not measured.".into()]),
        SceneCommitIntent::CaptionImport(input)=>(&input.sequence_id,if input.format==SubtitleFormat::Ass{vec!["ASS text and timing imported; styles are replaced by the explicitly selected pinned V4 style. ASS does not supply trusted font bytes.".into()]}else{vec![]}),
        _=>return Err(EditorError::new("INVALID_ARGUMENT","Caption receipt requires its original intent")),
    };
    let before =
        sqlite_scene_jobs::fixed(connection, intent.project_id(), &transaction.base_revision)?;
    let after = sqlite_scene_jobs::fixed(connection, intent.project_id(), &transaction.revision)?;
    let count = |snapshot: &EditorSnapshot| {
        snapshot
            .project
            .sequences
            .iter()
            .find(|s| &s.id == scene)
            .map(|s| s.captions.len())
            .ok_or_else(|| {
                EditorError::new("INVALID_STORED_RESULT", "Caption receipt scene is missing")
            })
    };
    let cue_count = count(&after)?
        .checked_sub(count(&before)?)
        .filter(|n| *n > 0 && *n <= 5000)
        .ok_or_else(|| {
            EditorError::new("INVALID_STORED_RESULT", "Caption receipt count is invalid")
        })?;
    Ok(CaptionReceipt {
        transaction: transaction.clone(),
        cue_count,
        warnings,
    })
}
