//! Caption generation commits through the existing visual journal in one transaction.
use super::*;
use opencut_api_contracts::subtitle_exchange::{GenerateCaptions, SubtitleImportReceipt};
use opencut_job_contracts::SequenceTranscribeResult;
use sqlite_project_v3::{EditCommit, commit_edit, load_v3_snapshot, require_edit};

impl EditorApplication {
    pub fn generate_captions(
        &mut self,
        input: &GenerateCaptions,
    ) -> Result<SubtitleImportReceipt, EditorError> {
        require_idempotency_key(&input.idempotency_key)?;
        let identity = request_hash("subtitle_generate", &input.project_id, input)?;
        let context = self.connection.1.clone();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        require_edit(&tx, &context, &input.project_id)?;
        sqlite_edit_control::require_writable(&tx, &context, &input.project_id)?;
        sqlite_access::require_job(&tx, &context, &input.analysis_job_id, ProjectAction::Read)?;
        if let Some(receipt) = load_idempotent(
            &tx,
            "subtitle_generate",
            &input.project_id,
            &input.idempotency_key,
            &identity,
            "transaction",
        )? {
            return decode_json(&receipt, "caption generation receipt");
        }
        let job =
            opencut_editor_application::job_views::project(load_job(&tx, &input.analysis_job_id)?);
        if job.kind != "analysis.sequence_transcribe"
            || job.state != JobState::Succeeded
            || job.result_unavailable.is_some()
        {
            return Err(EditorError::new(
                "ANALYSIS_NOT_READY",
                "A successful scene transcription is required",
            ));
        }
        let result = job.extended_result.ok_or_else(|| {
            EditorError::new("INVALID_STORED_RESULT", "Transcript is unavailable")
        })?;
        if result.kind != job.kind || result.schema_version != 1 {
            return Err(EditorError::new(
                "INVALID_STORED_RESULT",
                "Transcript version is unsupported",
            ));
        }
        let result: SequenceTranscribeResult = serde_json::from_value(result.data)
            .map_err(|_| EditorError::new("INVALID_STORED_RESULT", "Transcript is invalid"))?;
        let current = load_v3_snapshot(&tx, &input.project_id, None)?;
        let plan = opencut_editor_application::caption_generation::plan(&current, input, &result)?;
        let cue_count = plan
            .proposed_project
            .base
            .sequences
            .iter()
            .find(|scene| scene.id == input.sequence_id)
            .map(|scene| scene.caption_cues.len())
            .unwrap_or(0)
            - current
                .project
                .base
                .sequences
                .iter()
                .find(|scene| scene.id == input.sequence_id)
                .map(|scene| scene.caption_cues.len())
                .unwrap_or(0);
        let transaction = commit_edit(
            &tx,
            &context,
            &current,
            plan,
            EditCommit {
                project_id: &input.project_id,
                idempotency_key: &input.idempotency_key,
                identity: &identity,
                family: "subtitle_generate",
                event: "subtitle.generated",
                request: json!({"kind":"subtitle.generate", "request":input}),
            },
        )?;
        let receipt = SubtitleImportReceipt {
            transaction,
            cue_count,
            warnings: vec!["Caption timing is estimated within speech segments.".into()],
        };
        // commit_edit records a transaction projection; this operation returns a
        // richer receipt under the same protected identity and transaction.
        tx.execute("UPDATE idempotency SET result_json=?1 WHERE operation_family='subtitle_generate' AND scope=?2 AND idempotency_key=?3",
            params![encode_json(&receipt, "caption generation receipt")?,
                input.project_id, input.idempotency_key]).map_err(storage_error)?;
        tx.commit().map_err(storage_error)?;
        Ok(receipt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencut_contracts::project_v3_ops::{ApplyV3Project, CreateV3Project, InspectV3Project};
    fn snapshot(app: &EditorApplication) -> opencut_contracts::project_v3::ProjectSnapshotV3 {
        app.inspect_v3_project(&InspectV3Project {
            project_id: "p".into(),
            revision: None,
        })
        .unwrap()
    }
    fn setup() -> (EditorApplication, tempfile::TempDir, GenerateCaptions) {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("audio.wav");
        // Minimal PCM fixture; this test validates stored inference, not ASR accuracy.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&32036u32.to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&16000u32.to_le_bytes());
        bytes.extend_from_slice(&32000u32.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&32000u32.to_le_bytes());
        bytes.resize(32044, 0);
        std::fs::write(&source, bytes).unwrap();
        let mut app = EditorApplication::open(directory.path().join("workspace.sqlite")).unwrap();
        let created = app
            .create_v3_project(
                &serde_json::from_value::<CreateV3Project>(json!({"project":{
            "id":"p","name":"Speech","sequence_id":"scene","width":640,"height":360,
            "fps":{"numerator":25,"denominator":1}},"idempotency_key":"create"}))
                .unwrap(),
            )
            .unwrap();
        app.apply_v3_project(&serde_json::from_value::<ApplyV3Project>(json!({"project_id":"p",
            "expected_revision":created.revision,"idempotency_key":"audio","label":"Audio","operations":[
                {"type":"base.apply","operation":{"type":"asset.bind","asset":{"id":"a","name":"Speech",
                    "kind":"audio","uri":source,"duration":{"ticks":"1","timebase":1},"metadata":{}}}},
                {"type":"base.apply","operation":{"type":"track.create","sequence_id":"scene","track":{
                    "id":"audio","name":"Audio","kind":"audio","enabled":true,"muted":false,"locked":false,"clips":[]}}},
                {"type":"base.apply","operation":{"type":"clip.insert","sequence_id":"scene","track_id":"audio","clip":
                        {"id":"clip","asset_id":"a","timeline_range":{"start":{"ticks":"0","timebase":1},"duration":{"ticks":"1","timebase":1}},
                        "source_range":{"start":{"ticks":"0","timebase":1},"duration":{"ticks":"1","timebase":1}},"params":{},"channels":[]}}}
            ]})).unwrap()).unwrap();
        let current = snapshot(&app);
        let input = opencut_job_contracts::SequenceTranscribeInput {
            idempotency_key: "analysis".into(),
            project_id: "p".into(),
            revision: current.revision.clone(),
            sequence_id: "scene".into(),
            language: Some("en".into()),
        };
        let accepted =
            crate::sqlite_project_v3_jobs::start_sequence_transcribe(&mut app, input.clone())
                .unwrap();
        assert_eq!(
            crate::sqlite_project_v3_jobs::start_sequence_transcribe(&mut app, input)
                .unwrap()
                .job_id,
            accepted.job_id
        );
        let claimed = app.live_jobs().claim("fixture", 30000).unwrap().unwrap();
        assert_eq!(claimed.job_id, accepted.job_id);
        let result = json!({"project_id":"p","revision":current.revision,"sequence_id":"scene","duration":{"ticks":"1","timebase":1},
            "asset_lock":accepted.input["asset_lock"],"transcription":{"language":"en","provider":"whisper.cpp","model":"small",
            "segments":[{"start":{"ticks":"0","timebase":1000},"end":{"ticks":"1000","timebase":1000},
                "text":vec!["word";387].join(" "),"words":[]}]}});
        app.live_jobs()
            .complete(&accepted.job_id, "fixture", claimed.attempt, result)
            .unwrap();
        let request = GenerateCaptions {
            project_id: "p".into(),
            expected_revision: current.revision,
            sequence_id: "scene".into(),
            analysis_job_id: accepted.job_id,
            idempotency_key: "captions".into(),
            words_per_caption: 3,
            style: None,
        };
        (app, directory, request)
    }
    #[test]
    fn bulk_generation_is_atomic_idempotent_and_one_reversible_history_entry() {
        let (mut app, _directory, input) = setup();
        let before = snapshot(&app);
        let result = app.generate_captions(&input).unwrap();
        assert_eq!(result.cue_count, 129);
        assert_eq!(
            snapshot(&app).project.base.sequences[0].caption_cues.len(),
            129
        );
        assert_eq!(
            serde_json::to_value(app.generate_captions(&input).unwrap()).unwrap(),
            serde_json::to_value(&result).unwrap()
        );
        let mut mismatch = input.clone();
        mismatch.words_per_caption = 5;
        assert_eq!(
            app.generate_captions(&mismatch).unwrap_err().code,
            "IDEMPOTENCY_KEY_REUSED"
        );
        mismatch.idempotency_key = "stale".into();
        assert_eq!(
            app.generate_captions(&mismatch).unwrap_err().code,
            "REVISION_CONFLICT"
        );
        let undone = app
            .apply_v3_undo(
                &serde_json::from_value(
                    json!({"project_id":"p","expected_revision":result.transaction.revision,
            "target_transaction_id":result.transaction.transaction_id,"idempotency_key":"undo"}),
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(snapshot(&app).project, before.project);
        app.apply_v3_redo(
            &serde_json::from_value(
                json!({"project_id":"p","expected_revision":undone.transaction.revision,
            "undo_transaction_id":undone.transaction.transaction_id,"idempotency_key":"redo"}),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            snapshot(&app).project.base.sequences[0].caption_cues.len(),
            129
        );
    }
    #[test]
    fn matching_corrupt_lock_in_input_and_result_is_unavailable() {
        let (mut app, _directory, input) = setup();
        let before = snapshot(&app);
        app.connection.execute("UPDATE jobs SET input_json=json_set(input_json,'$.asset_lock.assets',json('[]')), result_json=json_set(result_json,'$.asset_lock.assets',json('[]')) WHERE job_id=?1", [&input.analysis_job_id]).unwrap();
        assert!(
            app.job_view(&input.analysis_job_id)
                .unwrap()
                .result_unavailable
                .is_some()
        );
        assert!(app.generate_captions(&input).is_err());
        assert_eq!(snapshot(&app), before);
    }
    #[test]
    fn unavailable_result_leaves_no_caption_revision() {
        let (mut app, _directory, input) = setup();
        let before = snapshot(&app);
        app.connection.execute("UPDATE jobs SET result_json=json_set(result_json,'$.project_id','other') WHERE job_id=?1",[&input.analysis_job_id]).unwrap();
        assert!(
            app.job_view(&input.analysis_job_id)
                .unwrap()
                .result_unavailable
                .is_some()
        );
        assert_eq!(
            app.generate_captions(&input).unwrap_err().code,
            "ANALYSIS_NOT_READY"
        );
        assert_eq!(snapshot(&app), before);
    }
    #[test]
    fn scene_jobs_and_caption_receipts_obey_current_grants_and_owner_holds() {
        use opencut_editor_application::access::*;
        let (mut app, _directory, input) = setup();
        let before = snapshot(&app);
        app.set_principal_policy(PrincipalPolicy {
            principal_id: "caption-editor".into(),
            expected_revision: "0".into(),
            enabled: true,
            grants: vec![ProjectGrant {
                project_id: "p".into(),
                actions: vec![
                    ProjectAction::Read,
                    ProjectAction::Edit,
                    ProjectAction::RunJobs,
                ],
            }],
        })
        .unwrap();
        let issued = app.issue_credential("caption-editor").unwrap();
        let caller = app.authenticate(&issued.secret).unwrap();
        app.set_edit_control(&serde_json::from_value(json!({"project_id":"p","expected_generation":"0","held":true,"idempotency_key":"hold"})).unwrap()).unwrap();
        app.set_caller(caller.clone());
        assert!(
            app.job_view(&input.analysis_job_id)
                .unwrap()
                .extended_result
                .is_some()
        );
        assert_eq!(
            app.generate_captions(&input).unwrap_err().code,
            "EDIT_HELD_BY_OWNER"
        );
        assert_eq!(snapshot(&app), before);
        app.set_caller(Caller::LocalOwner);
        app.set_edit_control(&serde_json::from_value(json!({"project_id":"p","expected_generation":"1","held":false,"idempotency_key":"release"})).unwrap()).unwrap();
        app.set_caller(caller.clone());
        let receipt = app.generate_captions(&input).unwrap();
        assert_eq!(receipt.cue_count, 129);
        app.set_caller(Caller::LocalOwner);
        app.set_principal_policy(PrincipalPolicy {
            principal_id: "caption-editor".into(),
            expected_revision: "1".into(),
            enabled: true,
            grants: vec![ProjectGrant {
                project_id: "p".into(),
                actions: vec![ProjectAction::Read],
            }],
        })
        .unwrap();
        app.set_caller(caller);
        assert_eq!(
            app.generate_captions(&input).unwrap_err().code,
            "NOT_AUTHORIZED"
        );
        let current = snapshot(&app);
        let refused = crate::sqlite_project_v3_jobs::start_sequence_transcribe(
            &mut app,
            opencut_job_contracts::SequenceTranscribeInput {
                project_id: "p".into(),
                revision: current.revision,
                sequence_id: "scene".into(),
                language: None,
                idempotency_key: "refused".into(),
            },
        )
        .unwrap_err();
        assert_eq!(refused.code, "NOT_AUTHORIZED");
    }
}
