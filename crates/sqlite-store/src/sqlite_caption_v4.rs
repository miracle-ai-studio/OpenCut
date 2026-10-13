//! Caption use cases reuse the V4 commit kernel and its original request receipts.
use super::*;
use opencut_api_contracts::{subtitle_exchange::SubtitleExportReceipt, subtitle_v4::*};
use opencut_contracts::{project_v4::ProjectSnapshotV4, project_v4_ops::ApplyV4Project};
use opencut_editor_application::{
    audit::ExecutionContext,
    project_v4::{self, ProjectV4Read, ProjectV4Store, V4Intent},
    sequence_transcribe_v4::PreparedSequenceTranscribeV4,
};
use opencut_job_contracts::sequence_transcribe_v4::*;

pub(super) fn require_generation_source(
    connection: &Connection,
    context: &ExecutionContext,
    input: &GenerateCaptionsV4,
) -> Result<SequenceTranscribeResultV4, EditorError> {
    sqlite_access::require_job(
        connection,
        context,
        &input.analysis_job_id,
        ProjectAction::Read,
    )?;
    let job = load_job(connection, &input.analysis_job_id)?;
    if job.kind != SEQUENCE_TRANSCRIBE_V4_KIND || job.state != JobState::Succeeded {
        return Err(EditorError::new(
            "ANALYSIS_NOT_READY",
            "A successful V4 whole-scene transcription is required",
        ));
    }
    let prepared: PreparedSequenceTranscribeV4 = serde_json::from_value(job.input.clone())
        .map_err(|_| EditorError::new("INVALID_STORED_RESULT", "Transcript source is invalid"))?;
    // A formerly authorized job cannot be used after the original caller loses
    // access. Completion validation also binds its exact snapshot/mix evidence.
    sqlite_sequence_transcribe_v4::require_completion(connection, &job)?;
    if prepared.audio.snapshot.project.id != input.project_id
        || prepared.audio.snapshot.revision != input.expected_revision
        || prepared.audio.sequence_id != input.sequence_id
    {
        return Err(EditorError::new(
            "REVISION_CONFLICT",
            "Caption generation requires the original transcribed V4 revision",
        ));
    }
    serde_json::from_value(
        job.result.ok_or_else(|| {
            EditorError::new("INVALID_STORED_RESULT", "Transcript is unavailable")
        })?,
    )
    .map_err(|_| EditorError::new("INVALID_STORED_RESULT", "Transcript is invalid"))
}
/// Called by the existing V4 commit kernel under its publication transaction.
/// Re-plan from the fixed immutable source to reject a tampered prepared edit.
pub(super) fn require_caption_proposal(
    connection: &Connection,
    context: &ExecutionContext,
    intent: V4Intent<'_>,
    proposed: &ProjectSnapshotV4,
) -> Result<(), EditorError> {
    let request = match intent {
        V4Intent::CaptionGenerate(input) => {
            let transcript = require_generation_source(connection, context, input)?;
            let fixed = sqlite_project_v4_jobs::fixed(
                connection,
                &input.project_id,
                &input.expected_revision,
            )?;
            opencut_editor_application::caption_v4::generate(&fixed, input, &transcript)?
        }
        V4Intent::CaptionImport(input) => {
            let fixed = sqlite_project_v4_jobs::fixed(
                connection,
                &input.project_id,
                &input.expected_revision,
            )?;
            opencut_editor_application::caption_v4::import(&fixed, input)?
        }
        _ => return Ok(()),
    };
    let fixed =
        sqlite_project_v4_jobs::fixed(connection, &request.project_id, &request.expected_revision)?;
    let expected = project_v4::plan(&fixed, &request)?;
    if proposed.project != expected {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Prepared captions differ from the original request",
        ));
    }
    Ok(())
}
fn cue_count(request: &ApplyV4Project, source: &ProjectSnapshotV4, sequence_id: &str) -> usize {
    let opencut_contracts::project_v4_ops::ProjectOperationV4::SequencePut { sequence } =
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
    pub fn generate_captions_v4(
        &mut self,
        input: &GenerateCaptionsV4,
    ) -> Result<CaptionReceiptV4, EditorError> {
        let intent = V4Intent::CaptionGenerate(input);
        require_idempotency_key(&input.idempotency_key)?;
        self.authorize_v4(intent)?;
        let transcript = {
            let tx = self
                .connection
                .unchecked_transaction()
                .map_err(storage_error)?;
            require_generation_source(&tx, &self.connection.1, input)?
        };
        let fixed = sqlite_project_v4_jobs::fixed(
            &self.connection,
            &input.project_id,
            &input.expected_revision,
        )?;
        let request = opencut_editor_application::caption_v4::generate(&fixed, input, &transcript)?;
        let count = cue_count(&request, &fixed, &input.sequence_id);
        let source = sqlite_project_v4::load_source(
            &self.connection,
            &input.project_id,
            Some(&input.expected_revision),
        )?;
        let proposed = project_v4::plan(&fixed, &request)?;
        let prepared = project_v4::prepare_caption_commit(&source, proposed, intent)?;
        let transaction = self.commit_v4_atomic(intent, prepared)?;
        Ok(CaptionReceiptV4{transaction,cue_count:count,warnings:vec!["Caption timing is estimated proportionally within locally inferred speech segments; word timestamps are not measured.".into()]})
    }
    pub fn import_subtitles_v4(
        &mut self,
        input: &SubtitleImportV4,
    ) -> Result<CaptionReceiptV4, EditorError> {
        let intent = V4Intent::CaptionImport(input);
        require_idempotency_key(&input.idempotency_key)?;
        self.authorize_v4(intent)?;
        let fixed = sqlite_project_v4_jobs::fixed(
            &self.connection,
            &input.project_id,
            &input.expected_revision,
        )?;
        let request = opencut_editor_application::caption_v4::import(&fixed, input)?;
        let count = cue_count(&request, &fixed, &input.sequence_id);
        let source = sqlite_project_v4::load_source(
            &self.connection,
            &input.project_id,
            Some(&input.expected_revision),
        )?;
        let proposed = project_v4::plan(&fixed, &request)?;
        let prepared = project_v4::prepare_caption_commit(&source, proposed, intent)?;
        let transaction = self.commit_v4_atomic(intent, prepared)?;
        Ok(CaptionReceiptV4 {
            transaction,
            cue_count: count,
            warnings: if input.format == SubtitleFormatV4::Ass {
                vec!["ASS text and timing imported; styles are replaced by the explicitly selected pinned V4 style. ASS does not supply trusted font bytes.".into()]
            } else {
                vec![]
            },
        })
    }
    pub fn export_subtitles_v4(
        &self,
        input: &SubtitleExportV4,
    ) -> Result<SubtitleExportReceipt, EditorError> {
        let fixed = project_v4::inspect(
            self,
            &opencut_contracts::project_v4_ops::InspectV4Project {
                project_id: input.project_id.clone(),
                revision: Some(input.revision.clone()),
            },
        )?;
        opencut_editor_application::caption_v4::export(&fixed, input)
    }
}

#[cfg(test)]
mod tests;

/// The complete public caption receipt is persisted in the same V4 commit lock.
pub(super) fn caption_receipt(
    connection: &Connection,
    intent: V4Intent<'_>,
    transaction: &TransactionResult,
) -> Result<CaptionReceiptV4, EditorError> {
    let (scene,warnings)=match intent{
        V4Intent::CaptionGenerate(input)=>(&input.sequence_id,vec!["Caption timing is estimated proportionally within locally inferred speech segments; word timestamps are not measured.".into()]),
        V4Intent::CaptionImport(input)=>(&input.sequence_id,if input.format==SubtitleFormatV4::Ass{vec!["ASS text and timing imported; styles are replaced by the explicitly selected pinned V4 style. ASS does not supply trusted font bytes.".into()]}else{vec![]}),
        _=>return Err(EditorError::new("INVALID_ARGUMENT","Caption receipt requires its original intent")),
    };
    let before =
        sqlite_project_v4_jobs::fixed(connection, intent.project_id(), &transaction.base_revision)?;
    let after =
        sqlite_project_v4_jobs::fixed(connection, intent.project_id(), &transaction.revision)?;
    let count = |snapshot: &ProjectSnapshotV4| {
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
    Ok(CaptionReceiptV4 {
        transaction: transaction.clone(),
        cue_count,
        warnings,
    })
}
