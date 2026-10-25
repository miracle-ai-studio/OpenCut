//! Durable reservations are released only after native staging deletion succeeds.
use super::sqlite_uploads::{authorized, event, permission};
use super::*;
use opencut_editor_application::uploads::*;
pub(super) const MIGRATION: &str = "
CREATE TABLE upload_retention(
 upload_id TEXT PRIMARY KEY REFERENCES upload_sessions(upload_id),
 reserved_bytes INTEGER NOT NULL CHECK(reserved_bytes>0),
 released INTEGER NOT NULL DEFAULT 0 CHECK(released IN (0,1)),
 released_at TEXT);
CREATE INDEX upload_retention_live_idx ON upload_retention(released,upload_id);
INSERT INTO upload_retention(upload_id,reserved_bytes) SELECT upload_id,size_bytes FROM upload_sessions;
UPDATE metadata SET value='9' WHERE key='schema_version' AND value='8';
";
fn used(c: &Connection, project: Option<&str>) -> Result<u64, EditorError> {
    c.query_row("SELECT COALESCE(SUM(r.reserved_bytes),0) FROM upload_retention r JOIN upload_sessions u USING(upload_id) WHERE r.released=0 AND (?1 IS NULL OR u.project_id=?1)", [project], |r|r.get(0)).map_err(storage_error)
}
pub(super) fn reserve(c: &Connection, input: &BeginUpload, id: &str) -> Result<(), EditorError> {
    let size = decimal(&input.size_bytes)?;
    if used(c, None)?
        .checked_add(size)
        .is_none_or(|n| n > i64::MAX as u64)
    {
        return Err(EditorError::new(
            "UPLOAD_RESERVATION_OVERFLOW",
            "Upload reservation accounting exceeds its integer addressability",
        ));
    }
    c.execute(
        "INSERT INTO upload_retention(upload_id,reserved_bytes) VALUES (?1,?2)",
        params![id, size],
    )
    .map_err(storage_error)?;
    Ok(())
}
fn view(c: &Connection, upload: &UploadView) -> Result<UploadRetentionView, EditorError> {
    let (size, released): (u64, bool) = c
        .query_row(
            "SELECT reserved_bytes,released FROM upload_retention WHERE upload_id=?1",
            [&upload.upload_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(storage_error)?;
    if size != decimal(&upload.size_bytes)? {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Upload reservation identity mismatch",
        ));
    }
    let eligible = match upload.state {
        UploadState::Aborted => true,
        UploadState::Receiving => false,
        UploadState::Sealed => {
            let id = upload
                .job_id
                .as_ref()
                .ok_or_else(|| EditorError::new("STORAGE_ERROR", "Sealed upload lacks a job"))?;
            let (kind, state): (String, String) = c
                .query_row("SELECT kind,state FROM jobs WHERE job_id=?1", [id], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })
                .map_err(storage_error)?;
            if kind != "media.import" {
                return Err(EditorError::new(
                    "STORAGE_ERROR",
                    "Sealed upload has a different job kind",
                ));
            }
            matches!(state.as_str(), "succeeded" | "failed" | "cancelled")
        }
    };
    Ok(UploadRetentionView {
        upload_id: upload.upload_id.clone(),
        project_id: upload.project_id.clone(),
        state: if released {
            UploadRetentionState::Released
        } else {
            UploadRetentionState::Retained
        },
        reserved_bytes: if released {
            "0".into()
        } else {
            size.to_string()
        },
        release_eligible: eligible,
    })
}
fn require_eligible(view: &UploadRetentionView) -> Result<(), EditorError> {
    if !view.release_eligible {
        return Err(EditorError::new(
            "UPLOAD_STATE_CONFLICT",
            "Only aborted or terminal imports can release staging",
        ));
    }
    Ok(())
}
fn retained_upload(
    c: &Connection,
    context: &opencut_editor_application::audit::ExecutionContext,
    id: &str,
    write: bool,
) -> Result<UploadView, EditorError> {
    if context.caller().is_owner() {
        return sqlite_uploads::read(c, id)?
            .ok_or_else(|| EditorError::new("NOT_FOUND", "Upload was not found"));
    }
    authorized(c, context, id, write)
}
impl UploadRetentionStore for EditorApplication {
    fn read_upload_page(
        &self,
        project: &str,
        retained_only: bool,
        upper: Option<&str>,
        after: Option<&str>,
        limit: usize,
    ) -> Result<UploadPageSlice, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        permission(&tx, &self.connection.1, project, false)?;
        if !tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM projects WHERE project_id=?1)",
                [project],
                |r| r.get::<_, bool>(0),
            )
            .map_err(storage_error)?
        {
            return Err(EditorError::new("NOT_FOUND", "Project was not found"));
        }
        let upper_id=match upper {
            Some(id)=>Some(id.to_owned()),
            None=>tx.query_row("SELECT upload_id FROM upload_sessions WHERE project_id=?1 ORDER BY rowid DESC LIMIT 1",[project],|r|r.get::<_,String>(0)).optional().map_err(storage_error)?,
        };
        let Some(upper_id) = upper_id else {
            return Ok(UploadPageSlice {
                records: vec![],
                upper_id: None,
                has_more: false,
            });
        };
        let rowid = |id: &str| -> Result<i64, EditorError> {
            tx.query_row(
                "SELECT rowid FROM upload_sessions WHERE upload_id=?1 AND project_id=?2",
                params![id, project],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage_error)?
            .ok_or_else(|| {
                EditorError::new("CURSOR_EXPIRED", "Upload cursor anchor is unavailable")
            })
        };
        let upper = rowid(&upper_id)?;
        let after = after.map(rowid).transpose()?.unwrap_or(0);
        if after > upper {
            return Err(EditorError::new(
                "INVALID_CURSOR",
                "Invalid upload page interval",
            ));
        }
        let ids: Vec<String> = {
            let mut query=tx.prepare("SELECT u.upload_id FROM upload_sessions u JOIN upload_retention r USING(upload_id) WHERE u.project_id=?1 AND u.rowid>?2 AND u.rowid<=?3 AND (?4=0 OR r.released=0) ORDER BY u.rowid LIMIT ?5").map_err(storage_error)?;
            query
                .query_map(
                    params![project, after, upper, retained_only, limit as i64 + 1],
                    |r| r.get(0),
                )
                .map_err(storage_error)?
                .collect::<Result<_, _>>()
                .map_err(storage_error)?
        };
        let has_more = ids.len() > limit;
        let records = ids
            .iter()
            .take(limit)
            .map(|id| {
                let upload = super::sqlite_uploads::read(&tx, id)?.ok_or_else(|| {
                    EditorError::new("STORAGE_ERROR", "Upload disappeared in read snapshot")
                })?;
                let retention = view(&tx, &upload)?;
                Ok(UploadEntry { upload, retention })
            })
            .collect::<Result<_, EditorError>>()?;
        tx.commit().map_err(storage_error)?;
        Ok(UploadPageSlice {
            records,
            upper_id: Some(upper_id),
            has_more,
        })
    }

    fn upload_retention(&self, id: &str) -> Result<UploadRetentionView, EditorError> {
        validate_id(id)?;
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let upload = retained_upload(&tx, &self.connection.1, id, false)?;
        let result = view(&tx, &upload)?;
        tx.commit().map_err(storage_error)?;
        Ok(result)
    }
    fn upload_quota(&self, project: &str) -> Result<UploadQuotaView, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        permission(&tx, &self.connection.1, project, false)?;
        if !tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM projects WHERE project_id=?1)",
                [project],
                |r| r.get::<_, bool>(0),
            )
            .map_err(storage_error)?
        {
            return Err(EditorError::new("NOT_FOUND", "Project was not found"));
        }
        let result = UploadQuotaView {
            project_id: project.into(),
            project_reserved_bytes: used(&tx, Some(project))?.to_string(),
        };
        tx.commit().map_err(storage_error)?;
        Ok(result)
    }
    fn authorize_upload_release(&self, id: &str) -> Result<UploadRetentionView, EditorError> {
        validate_id(id)?;
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let upload = retained_upload(&tx, &self.connection.1, id, true)?;
        let result = view(&tx, &upload)?;
        require_eligible(&result)?;
        tx.commit().map_err(storage_error)?;
        Ok(result)
    }
    fn finish_upload_release(&mut self, id: &str) -> Result<UploadRetentionView, EditorError> {
        validate_id(id)?;
        let ctx = self.connection.1.clone();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let upload = retained_upload(&tx, &ctx, id, true)?;
        let before = view(&tx, &upload)?;
        require_eligible(&before)?;
        if before.state == UploadRetentionState::Released {
            return Ok(before);
        }
        tx.execute("UPDATE upload_retention SET released=1,released_at=CURRENT_TIMESTAMP WHERE upload_id=?1",[id]).map_err(storage_error)?;
        event(&tx, &ctx, &upload, "upload.staging_released", None)?;
        let result = view(&tx, &upload)?;
        tx.commit().map_err(storage_error)?;
        Ok(result)
    }
}
