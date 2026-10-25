//! Authenticated host-only request slot, separate from content-write recovery:
//! playback needs Read + RunJobs, never an edit lease or project mutation.
use super::sqlite_edit_recovery_shared::{Scope, scope};
use super::*;
use opencut_editor_application::audio_preview_recovery::{self as recovery, *};
#[cfg(test)]
mod tests;

pub(super) const MIGRATION: &str = "
CREATE TABLE audio_preview_recovery_v1(
 recovery_id TEXT PRIMARY KEY,
 project_id TEXT NOT NULL REFERENCES projects(project_id) ON DELETE CASCADE,
 principal_id TEXT NOT NULL,credential_id TEXT NOT NULL,
 owner INTEGER NOT NULL CHECK(owner IN(0,1)),
 request_digest TEXT NOT NULL,payload TEXT NOT NULL,
 stored_bytes INTEGER NOT NULL CHECK(stored_bytes>0 AND stored_bytes<=16384),
 host_instance TEXT NOT NULL,status TEXT NOT NULL CHECK(status IN('staged','completed','retired')),
 receipt TEXT,dispatched INTEGER NOT NULL DEFAULT 0 CHECK(dispatched IN(0,1)),
 UNIQUE(project_id,principal_id,credential_id,owner)
);
UPDATE metadata SET value='31' WHERE key='schema_version' AND value='30';
";
fn require(
    c: &Connection,
    context: &opencut_editor_application::audit::ExecutionContext,
    project: &str,
) -> Result<(), EditorError> {
    recovery::identifier(project)?;
    sqlite_scene_jobs::require_scope(c, context, project)
}
fn load(
    c: &Connection,
    identity: &Scope,
    project: &str,
    id: Option<&str>,
) -> Result<Option<StoredAudioPreviewRecovery>, EditorError> {
    let raw:Option<(String,String,String,String,String,Option<String>,bool)>=c.query_row("SELECT recovery_id,request_digest,payload,host_instance,status,receipt,dispatched FROM audio_preview_recovery_v1 WHERE project_id=?1 AND principal_id=?2 AND credential_id=?3 AND owner=?4 AND (?5 IS NULL OR recovery_id=?5)",params![project,identity.principal,identity.credential,identity.owner,id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).optional().map_err(storage_error)?;
    let Some((id, digest, payload, host_instance, status, receipt, dispatched)) = raw else {
        return Ok(None);
    };
    if payload.len() > MAX_RECORD_BYTES || recovery::digest(&payload) != digest {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Stored preview recovery identity differs",
        ));
    }
    let original: StageAudioPreviewRecovery =
        decode_json(&payload, "Original preview recovery request")?;
    let create = recovery::create(&original.request);
    if create.project_id != project {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Stored preview recovery scope differs",
        ));
    }
    let status = match status.as_str() {
        "staged" => AudioPreviewRecoveryStatus::Staged,
        "completed" => AudioPreviewRecoveryStatus::Completed,
        "retired" => AudioPreviewRecoveryStatus::Retired,
        _ => {
            return Err(EditorError::new(
                "STORAGE_ERROR",
                "Invalid preview recovery status",
            ));
        }
    };
    Ok(Some(StoredAudioPreviewRecovery {
        view: AudioPreviewRecoveryRecord {
            recovery_id: id,
            project_id: project.into(),
            revision: create.revision.clone(),
            sequence_id: create.sequence_id.clone(),
            request_digest: digest,
            kind: recovery::kind(&original.request),
            status,
        },
        original,
        host_instance,
        receipt: receipt.map(|r| decode_receipt(&r)).transpose()?,
        dispatched,
    }))
}
impl AudioPreviewRecoveryStore for EditorApplication {
    fn stage_audio_preview_recovery(
        &mut self,
        input: &StageAudioPreviewRecovery,
        host_instance: &str,
    ) -> Result<AudioPreviewRecoveryRecord, EditorError> {
        let raw = recovery::payload(input)?;
        let digest = recovery::digest(&raw);
        let create = recovery::create(&input.request);
        self.revalidate_scene_audio_preview(&create.project_id, &create.revision)?;
        let context = self.connection.1.clone();
        let identity = scope(&context);
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        require(&tx, &context, &create.project_id)?;
        if let Some(old) = load(&tx, &identity, &create.project_id, None)? {
            if old.view.request_digest != digest {
                return Err(EditorError::new(
                    "RECOVERY_CONFLICT",
                    "Resolve the original preview request before replacing it",
                ));
            }
            tx.commit().map_err(storage_error)?;
            return Ok(old.view);
        }
        let count: u64 = tx
            .query_row("SELECT COUNT(*) FROM audio_preview_recovery_v1", [], |r| {
                r.get(0)
            })
            .map_err(storage_error)?;
        if count >= MAX_RECORDS {
            return Err(EditorError::new(
                "RECOVERY_QUOTA_EXCEEDED",
                "At most 32 unresolved preview requests are retained",
            ));
        }
        let id = uuid::Uuid::new_v4().to_string();
        tx.execute("INSERT INTO audio_preview_recovery_v1(recovery_id,project_id,principal_id,credential_id,owner,request_digest,payload,stored_bytes,host_instance,status) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,'staged')",params![id,create.project_id,identity.principal,identity.credential,identity.owner,digest,raw,raw.len() as u64,host_instance]).map_err(storage_error)?;
        let record = load(&tx, &identity, &create.project_id, Some(&id))?
            .ok_or_else(|| {
                EditorError::new("STORAGE_ERROR", "Preview recovery slot was not stored")
            })?
            .view;
        tx.commit().map_err(storage_error)?;
        Ok(record)
    }
    fn read_audio_preview_recovery(
        &self,
        project: &str,
        id: Option<&str>,
    ) -> Result<Option<StoredAudioPreviewRecovery>, EditorError> {
        if let Some(id) = id {
            recovery::identifier(id)?;
        }
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        require(&tx, &self.connection.1, project)?;
        let result = load(&tx, &scope(&self.connection.1), project, id)?;
        if let Some(record) = &result {
            sqlite_scene_jobs::fixed(&tx, project, &record.view.revision)?;
        }
        Ok(result)
    }
    fn begin_audio_preview_recovery(&mut self, project: &str, id: &str) -> Result<(), EditorError> {
        let context = self.connection.1.clone();
        let identity = scope(&context);
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        require(&tx, &context, project)?;
        let record = load(&tx, &identity, project, Some(id))?.ok_or_else(|| {
            EditorError::new("NOT_FOUND", "Original preview recovery slot is absent")
        })?;
        if record.view.status != AudioPreviewRecoveryStatus::Staged {
            return Err(EditorError::new(
                "PREVIEW_RECOVERY_RETIRED",
                "Original preview request is no longer dispatchable",
            ));
        }
        tx.execute(
            "UPDATE audio_preview_recovery_v1 SET dispatched=1 WHERE recovery_id=?1",
            params![id],
        )
        .map_err(storage_error)?;
        tx.commit().map_err(storage_error)
    }
    fn complete_audio_preview_recovery(
        &mut self,
        project: &str,
        id: &str,
        receipt: &opencut_editor_application::audio_preview::AudioPreviewSession,
    ) -> Result<(), EditorError> {
        let context = self.connection.1.clone();
        let identity = scope(&context);
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        require(&tx, &context, project)?;
        let record = load(&tx, &identity, project, Some(id))?.ok_or_else(|| {
            EditorError::new("NOT_FOUND", "Original preview recovery slot is absent")
        })?;
        let create = recovery::create(&record.original.request);
        if receipt.identity.project_id != project
            || receipt.identity.revision != create.revision
            || receipt.identity.sequence_id != create.sequence_id
            || receipt.identity.sample_rate != create.sample_rate
            || record.view.status != AudioPreviewRecoveryStatus::Staged
        {
            return Err(EditorError::new(
                "PREVIEW_RECOVERY_RETIRED",
                "Original preview request is no longer dispatchable",
            ));
        }
        let receipt = serde_json::to_string(receipt).map_err(|_| {
            EditorError::new(
                "SERIALIZATION_ERROR",
                "Preview receipt could not be retained",
            )
        })?;
        tx.execute("UPDATE audio_preview_recovery_v1 SET status='completed',receipt=?2 WHERE recovery_id=?1",params![id,receipt]).map_err(storage_error)?;
        tx.commit().map_err(storage_error)
    }
    fn retire_audio_preview_recovery(
        &mut self,
        project: &str,
        id: &str,
    ) -> Result<(), EditorError> {
        let context = self.connection.1.clone();
        let identity = scope(&context);
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        require(&tx, &context, project)?;
        if load(&tx, &identity, project, Some(id))?.is_some() {
            tx.execute(
                "UPDATE audio_preview_recovery_v1 SET status='retired' WHERE recovery_id=?1",
                params![id],
            )
            .map_err(storage_error)?;
        }
        tx.commit().map_err(storage_error)
    }
    fn ack_audio_preview_recovery(&mut self, project: &str, id: &str) -> Result<(), EditorError> {
        let context = self.connection.1.clone();
        let identity = scope(&context);
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        require(&tx, &context, project)?;
        if let Some(record) = load(&tx, &identity, project, Some(id))? {
            if record.view.status == AudioPreviewRecoveryStatus::Staged {
                return Err(EditorError::new(
                    "RECOVERY_UNCONFIRMED",
                    "Retire the original preview request before acknowledging it",
                ));
            }
            tx.execute(
                "DELETE FROM audio_preview_recovery_v1 WHERE recovery_id=?1",
                params![id],
            )
            .map_err(storage_error)?;
        }
        tx.commit().map_err(storage_error)
    }
}

/// Historical receipts retain their exact stored payload. Normalize only its known
/// envelope after the original recovery digest/scope has already been checked.
/// Old submission receipts never become a fabricated device-output witness.
fn decode_receipt(
    raw: &str,
) -> Result<opencut_api_contracts::audio_preview::AudioPreviewSession, EditorError> {
    let mut value: serde_json::Value = decode_json(raw, "Original preview recovery receipt")?;
    if let Some(tag) = value
        .as_object_mut()
        .and_then(|object| object.remove("schema_version"))
    {
        if tag != serde_json::json!(1) {
            return Err(EditorError::new(
                "STORAGE_ERROR",
                "Invalid stored preview receipt format",
            ));
        }
    }
    if let Some(object) = value.as_object_mut() {
        if object.contains_key("authoritative_time") {
            if ["submitted_time", "presentation_time", "output"]
                .iter()
                .any(|key| object.contains_key(*key))
            {
                return Err(EditorError::new(
                    "STORAGE_ERROR",
                    "Mixed stored preview clock formats",
                ));
            }
            let submitted = object.remove("authoritative_time").unwrap();
            let start = object
                .get("actual_start_sample")
                .cloned()
                .ok_or_else(|| EditorError::new("STORAGE_ERROR", "Missing preview anchor"))?;
            let rate = object
                .get("identity")
                .and_then(|value| value.get("sample_rate"))
                .cloned()
                .ok_or_else(|| EditorError::new("STORAGE_ERROR", "Missing preview sample rate"))?;
            object.insert("submitted_time".into(), submitted);
            object.insert(
                "presentation_time".into(),
                serde_json::json!({ "ticks": start, "timebase": rate }),
            );
            object.insert("output".into(), serde_json::Value::Null);
            object.insert("presentation_fresh".into(), serde_json::Value::Bool(false));
        }
    }
    serde_json::from_value(value)
        .map_err(|_| EditorError::new("STORAGE_ERROR", "Invalid stored preview recovery receipt"))
}
