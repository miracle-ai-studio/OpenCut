//! Upload offsets are acknowledged only after native storage reports durable bytes.
use super::*;
use opencut_editor_application::{
    audit::ExecutionContext,
    uploads::{self, *},
};
pub(super) const MIGRATION: &str = "
CREATE TABLE upload_sessions(
 upload_id TEXT PRIMARY KEY,project_id TEXT NOT NULL REFERENCES projects(project_id),
 idempotency_key TEXT NOT NULL,request_hash TEXT NOT NULL,name TEXT NOT NULL,
 size_bytes INTEGER NOT NULL,content_hash TEXT NOT NULL,next_offset INTEGER NOT NULL DEFAULT 0,
 state TEXT NOT NULL DEFAULT 'receiving',job_id TEXT REFERENCES jobs(job_id),
 created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,UNIQUE(project_id,idempotency_key));
CREATE INDEX upload_project_state_idx ON upload_sessions(project_id,state);
CREATE TABLE upload_chunks(
 upload_id TEXT NOT NULL REFERENCES upload_sessions(upload_id),offset_bytes INTEGER NOT NULL,
 size_bytes INTEGER NOT NULL,content_hash TEXT NOT NULL,request_id TEXT NOT NULL,
 PRIMARY KEY(upload_id,offset_bytes));
CREATE TABLE project_media(
 media_id TEXT PRIMARY KEY,project_id TEXT NOT NULL REFERENCES projects(project_id),
 upload_id TEXT NOT NULL UNIQUE REFERENCES upload_sessions(upload_id),
 job_id TEXT NOT NULL REFERENCES jobs(job_id),asset_json TEXT NOT NULL,
 created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
CREATE INDEX project_media_project_idx ON project_media(project_id,media_id);
UPDATE metadata SET value='8' WHERE key='schema_version' AND value='7';
";
pub(super) fn permission(
    c: &Connection,
    ctx: &ExecutionContext,
    project: &str,
    write: bool,
) -> Result<(), EditorError> {
    sqlite_access::require(c, ctx, project, ProjectAction::Read)?;
    if write {
        for action in [ProjectAction::Edit, ProjectAction::RunJobs] {
            sqlite_access::require(c, ctx, project, action)?;
        }
    }
    Ok(())
}
pub(super) fn read(c: &Connection, id: &str) -> Result<Option<UploadView>, EditorError> {
    type UploadRow = (String, String, i64, String, i64, String, Option<String>);
    let row: Option<UploadRow> = c.query_row("SELECT project_id,name,size_bytes,content_hash,next_offset,state,job_id FROM upload_sessions WHERE upload_id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).optional().map_err(storage_error)?;
    row.map(
        |(project_id, name, size, content_hash, next, state, job_id)| {
            let state = match state.as_str() {
                "receiving" => UploadState::Receiving,
                "sealed" => UploadState::Sealed,
                "aborted" => UploadState::Aborted,
                _ => return Err(EditorError::new("STORAGE_ERROR", "Unknown upload state")),
            };
            Ok(UploadView {
                upload_id: id.into(),
                project_id,
                name,
                size_bytes: size.to_string(),
                content_hash,
                next_offset: next.to_string(),
                state,
                job_id,
            })
        },
    )
    .transpose()
}
pub(super) fn authorized(
    c: &Connection,
    ctx: &ExecutionContext,
    id: &str,
    write: bool,
) -> Result<UploadView, EditorError> {
    let view = read(c, id)?.ok_or_else(|| {
        if ctx.caller().is_owner() {
            EditorError::new("NOT_FOUND", "Upload was not found")
        } else {
            access::denied()
        }
    })?;
    permission(c, ctx, &view.project_id, write)?;
    Ok(view)
}
pub(super) fn event(
    c: &Connection,
    ctx: &ExecutionContext,
    view: &UploadView,
    action: &str,
    media_id: Option<&str>,
) -> Result<(), EditorError> {
    let snapshot = load_snapshot(c, &view.project_id)?;
    let payload = serde_json::json!({"project_id":view.project_id,"upload_id":view.upload_id,"next_offset":view.next_offset,"state":view.state,"job_id":view.job_id,"media_id":media_id});
    c.execute("INSERT INTO outbox(event_id,project_id,revision,event_type,payload_json) VALUES (?1,?2,?3,?4,?5)",params![format!("evt_{}",Uuid::new_v4().simple()),view.project_id,snapshot.revision,action,encode_json(&payload,"upload event")?]).map_err(storage_error)?;
    let origin: Option<String> = match view.job_id.as_deref() {
        Some(id) => c.query_row("SELECT request_id FROM audit_records WHERE job_id=?1 AND action='job.accepted' ORDER BY sequence LIMIT 1", [id], |r| r.get(0)).optional().map_err(storage_error)?.or_else(|| Some(ctx.request_id().into())),
        None => None,
    };
    sqlite_audit::insert(
        c,
        ctx,
        action,
        sqlite_audit::Links {
            project_id: Some(&view.project_id),
            job_id: view.job_id.as_deref(),
            origin_request_id: origin.as_deref(),
            ..Default::default()
        },
    )
}
fn checked_chunk(
    c: &Connection,
    ctx: &ExecutionContext,
    input: &ChunkDescriptor,
) -> Result<(UploadView, ChunkPosition), EditorError> {
    uploads::validate_chunk(input)?;
    let view = authorized(c, ctx, &input.upload_id, true)?;
    let position = uploads::chunk_position(&view, input)?;
    if position == ChunkPosition::Replay {
        let previous:Option<(i64,String)>=c.query_row("SELECT size_bytes,content_hash FROM upload_chunks WHERE upload_id=?1 AND offset_bytes=?2",params![input.upload_id,uploads::decimal(&input.offset)?],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(storage_error)?;
        if previous
            != Some((
                uploads::decimal(&input.size_bytes)? as i64,
                input.content_hash.clone(),
            ))
        {
            return Err(EditorError::new(
                "UPLOAD_CHUNK_CONFLICT",
                "A different chunk was confirmed at this offset",
            ));
        }
    } else {
        let count: i64 = c
            .query_row(
                "SELECT count(*) FROM upload_chunks WHERE upload_id=?1",
                [&input.upload_id],
                |r| r.get(0),
            )
            .map_err(storage_error)?;
        if count >= uploads::MAX_UPLOAD_CHUNKS {
            return Err(EditorError::new(
                "RESOURCE_TOO_LARGE",
                "Upload chunk limit reached; abort and restart with larger chunks",
            ));
        }
    }
    Ok((view, position))
}
impl UploadStore for EditorApplication {
    fn create_upload(&mut self, input: &CreateUpload) -> Result<UploadView, EditorError> {
        uploads::validate_create(input)?;
        let ctx = self.connection.1.clone();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        permission(&tx, &ctx, &input.project_id, true)?;
        load_snapshot(&tx, &input.project_id)?;
        let identity = request_hash("create_upload", &input.project_id, input)?;
        let replay:Option<(String,String)>=tx.query_row("SELECT upload_id,request_hash FROM upload_sessions WHERE project_id=?1 AND idempotency_key=?2",params![input.project_id,input.idempotency_key],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(storage_error)?;
        if let Some((id, hash)) = replay {
            if !identity.matches(&hash, None) {
                return Err(EditorError::new(
                    "IDEMPOTENCY_KEY_REUSED",
                    "Upload key was used with different parameters",
                ));
            }
            return authorized(&tx, &ctx, &id, true);
        }
        let project_count: i64 = tx
            .query_row(
                "SELECT count(*) FROM upload_sessions WHERE project_id=?1 AND state='receiving'",
                [&input.project_id],
                |r| r.get(0),
            )
            .map_err(storage_error)?;
        let workspace_count: i64 = tx
            .query_row(
                "SELECT count(*) FROM upload_sessions WHERE state='receiving'",
                [],
                |r| r.get(0),
            )
            .map_err(storage_error)?;
        if project_count >= 8 || workspace_count >= 128 {
            return Err(EditorError::new(
                "RESOURCE_TOO_LARGE",
                "Too many receiving uploads; finish or abort an existing upload",
            ));
        }
        let id = format!("upload_{}", Uuid::new_v4().simple());
        tx.execute("INSERT INTO upload_sessions(upload_id,project_id,idempotency_key,request_hash,name,size_bytes,content_hash) VALUES (?1,?2,?3,?4,?5,?6,?7)",params![id,input.project_id,input.idempotency_key,identity.current_digest(),input.name,uploads::decimal(&input.size_bytes)?,input.content_hash]).map_err(storage_error)?;
        sqlite_upload_retention::reserve(&tx, input, &id)?;
        let view = authorized(&tx, &ctx, &id, true)?;
        event(&tx, &ctx, &view, "upload.created", None)?;
        tx.commit().map_err(storage_error)?;
        Ok(view)
    }
    fn inspect_upload(&self, id: &str) -> Result<UploadView, EditorError> {
        uploads::validate_id(id)?;
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let view = authorized(&tx, &self.connection.1, id, false)?;
        tx.commit().map_err(storage_error)?;
        Ok(view)
    }
    fn authorize_chunk(&self, input: &ChunkDescriptor) -> Result<UploadView, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let (view, _) = checked_chunk(&tx, &self.connection.1, input)?;
        tx.commit().map_err(storage_error)?;
        Ok(view)
    }
    fn acknowledge_chunk(&mut self, input: &ChunkDescriptor) -> Result<UploadView, EditorError> {
        let ctx = self.connection.1.clone();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let (mut view, position) = checked_chunk(&tx, &ctx, input)?;
        if position == ChunkPosition::Replay {
            return Ok(view);
        }
        let offset = uploads::decimal(&input.offset)?;
        let size = uploads::decimal(&input.size_bytes)?;
        tx.execute("INSERT INTO upload_chunks(upload_id,offset_bytes,size_bytes,content_hash,request_id) VALUES (?1,?2,?3,?4,?5)",params![input.upload_id,offset,size,input.content_hash,ctx.request_id()]).map_err(storage_error)?;
        tx.execute(
            "UPDATE upload_sessions SET next_offset=?1 WHERE upload_id=?2",
            params![offset + size, input.upload_id],
        )
        .map_err(storage_error)?;
        view.next_offset = (offset + size).to_string();
        event(&tx, &ctx, &view, "upload.chunk_accepted", None)?;
        tx.commit().map_err(storage_error)?;
        Ok(view)
    }
    fn abort_upload(&mut self, id: &str) -> Result<UploadView, EditorError> {
        uploads::validate_id(id)?;
        let ctx = self.connection.1.clone();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let mut view = authorized(&tx, &ctx, id, true)?;
        match view.state {
            UploadState::Aborted => return Ok(view),
            UploadState::Sealed => {
                return Err(EditorError::new(
                    "UPLOAD_STATE_CONFLICT",
                    "Cancel the accepted import job explicitly",
                ));
            }
            UploadState::Receiving => {}
        }
        tx.execute(
            "UPDATE upload_sessions SET state='aborted' WHERE upload_id=?1",
            [id],
        )
        .map_err(storage_error)?;
        view.state = UploadState::Aborted;
        event(&tx, &ctx, &view, "upload.aborted", None)?;
        tx.commit().map_err(storage_error)?;
        Ok(view)
    }
}

fn prepared(
    c: &Connection,
    view: &UploadView,
    media_id: String,
) -> Result<PreparedMediaImportInput, EditorError> {
    let mut statement = c.prepare("SELECT offset_bytes,size_bytes,content_hash FROM upload_chunks WHERE upload_id=?1 ORDER BY offset_bytes LIMIT 4097").map_err(storage_error)?;
    let chunks = statement
        .query_map([&view.upload_id], |r| {
            Ok(PreparedUploadChunk {
                offset: r.get(0)?,
                size_bytes: r.get(1)?,
                content_hash: r.get(2)?,
            })
        })
        .map_err(storage_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(storage_error)?;
    let input = PreparedMediaImportInput {
        upload_id: view.upload_id.clone(),
        project_id: view.project_id.clone(),
        media_id,
        name: view.name.clone(),
        size_bytes: uploads::decimal(&view.size_bytes)?,
        content_hash: view.content_hash.clone(),
        chunks,
    };
    uploads::validate_manifest(&input)?;
    Ok(input)
}
impl ImportStore for EditorApplication {
    fn seal_upload(&mut self, id: &str) -> Result<UploadView, EditorError> {
        uploads::validate_id(id)?;
        let ctx = self.connection.1.clone();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let mut view = authorized(&tx, &ctx, id, true)?;
        match view.state {
            UploadState::Sealed => return Ok(view),
            UploadState::Aborted => {
                return Err(EditorError::new(
                    "UPLOAD_STATE_CONFLICT",
                    "Aborted upload cannot be imported",
                ));
            }
            UploadState::Receiving => {}
        }
        if view.next_offset != view.size_bytes {
            return Err(EditorError::new(
                "UPLOAD_INCOMPLETE",
                "Upload all declared bytes before sealing",
            ));
        }
        let input = prepared(&tx, &view, format!("media_{}", Uuid::new_v4().simple()))?;
        let snapshot = load_snapshot(&tx, &view.project_id)?;
        let record = JobRecord {
            job_id: format!("job_{}", Uuid::new_v4().simple()),
            kind: "media.import".into(),
            input: json!(input),
            state: JobState::Queued,
            attempt: 0,
            result: None,
            error: None,
            lease_owner: None,
            lease_until: None,
            cancel_requested: false,
            progress: None,
        };
        tx.execute("INSERT INTO jobs(job_id,state,attempt,kind,input_json,cancel_requested) VALUES (?1,'queued',0,'media.import',?2,0)",params![record.job_id,encode_json(&record.input,"sealed upload")?]).map_err(storage_error)?;
        // This revision is an authorization/audit anchor; import does not edit it.
        tx.execute(
            "INSERT INTO job_projects(job_id,project_id,revision) VALUES (?1,?2,?3)",
            params![record.job_id, view.project_id, snapshot.revision],
        )
        .map_err(storage_error)?;
        tx.execute(
            "UPDATE upload_sessions SET state='sealed',job_id=?1 WHERE upload_id=?2",
            params![record.job_id, id],
        )
        .map_err(storage_error)?;
        view.state = UploadState::Sealed;
        view.job_id = Some(record.job_id.clone());
        event(&tx, &ctx, &view, "upload.sealed", None)?;
        insert_job_event(&tx, &record, "queued", json!({}))?;
        sqlite_audit::job(&tx, &ctx, &record, "job.accepted", true)?;
        tx.commit().map_err(storage_error)?;
        Ok(view)
    }
    fn project_media(
        &self,
        input: &ProjectMediaInput,
    ) -> Result<MediaImportJobResult, EditorError> {
        uploads::validate_media_id(&input.media_id)?;
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        permission(&tx, &self.connection.1, &input.project_id, false)?;
        let result = media(&tx, &input.project_id, &input.media_id)?.ok_or_else(|| {
            if self.connection.1.caller().is_owner() {
                EditorError::new("NOT_FOUND", "Project media was not found")
            } else {
                access::denied()
            }
        })?;
        tx.commit().map_err(storage_error)?;
        Ok(result)
    }
}
pub(super) fn media(
    c: &Connection,
    project: &str,
    media_id: &str,
) -> Result<Option<MediaImportJobResult>, EditorError> {
    let row: Option<(String,String,String)> = c.query_row("SELECT upload_id,job_id,asset_json FROM project_media WHERE media_id=?1 AND project_id=?2",params![media_id,project],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(storage_error)?;
    row.map(|(upload_id, job_id, asset)| {
        Ok(MediaImportJobResult {
            media_id: media_id.into(),
            project_id: project.into(),
            upload_id,
            job_id,
            asset: decode_json(&asset, "project media")?,
        })
    })
    .transpose()
}
/// Called only after a successful attempt transition, in the same write transaction.
pub(super) fn publish_import(
    c: &Connection,
    context: &ExecutionContext,
    record: &JobRecord,
) -> Result<(), EditorError> {
    if record.kind != "media.import" || record.state != JobState::Succeeded {
        return Ok(());
    }
    let input: PreparedMediaImportInput = serde_json::from_value(record.input.clone())
        .map_err(|_| EditorError::new("INVALID_JOB_RESULT", "Invalid sealed import input"))?;
    let result: MediaImportJobResult =
        serde_json::from_value(record.result.clone().unwrap_or(Value::Null))
            .map_err(|_| EditorError::new("INVALID_JOB_RESULT", "Invalid imported media result"))?;
    uploads::validate_import_result(&input, &record.job_id, &result)?;
    let view = read(c, &input.upload_id)?
        .ok_or_else(|| EditorError::new("INVALID_JOB_RESULT", "Import upload is absent"))?;
    if view.state != UploadState::Sealed
        || view.job_id.as_deref() != Some(record.job_id.as_str())
        || prepared(c, &view, input.media_id.clone())? != input
    {
        return Err(EditorError::new(
            "INVALID_JOB_RESULT",
            "Import no longer matches its authoritative upload",
        ));
    }
    c.execute("INSERT INTO project_media(media_id,project_id,upload_id,job_id,asset_json) VALUES (?1,?2,?3,?4,?5)",params![result.media_id,result.project_id,result.upload_id,result.job_id,encode_json(&result.asset,"imported asset")?]).map_err(storage_error)?;
    event(c, context, &view, "media.imported", Some(&result.media_id))?;
    Ok(())
}

impl ProjectMediaRead for EditorApplication {
    fn read_media_page(
        &self,
        project: &str,
        upper: Option<&str>,
        after: Option<&str>,
        limit: usize,
    ) -> Result<MediaPageSlice, EditorError> {
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
        let upper_id = match upper {
            Some(id) => Some(id.to_owned()),
            None => tx.query_row("SELECT media_id FROM project_media WHERE project_id=?1 ORDER BY rowid DESC LIMIT 1",[project],|r|r.get::<_,String>(0)).optional().map_err(storage_error)?,
        };
        let Some(upper_id) = upper_id else {
            tx.commit().map_err(storage_error)?;
            return Ok(MediaPageSlice {
                records: vec![],
                upper_id: None,
                has_more: false,
            });
        };
        let rowid = |id: &str| -> Result<i64, EditorError> {
            tx.query_row(
                "SELECT rowid FROM project_media WHERE media_id=?1 AND project_id=?2",
                params![id, project],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage_error)?
            .ok_or_else(|| {
                EditorError::new(
                    "CURSOR_EXPIRED",
                    "Project media cursor anchor is unavailable",
                )
            })
        };
        let upper = rowid(&upper_id)?;
        let after = after.map(rowid).transpose()?.unwrap_or(0);
        if after > upper {
            return Err(EditorError::new(
                "INVALID_CURSOR",
                "Invalid media page interval",
            ));
        }
        let ids: Vec<String> = {
            let mut statement = tx.prepare("SELECT media_id FROM project_media WHERE project_id=?1 AND rowid>?2 AND rowid<=?3 ORDER BY rowid LIMIT ?4").map_err(storage_error)?;
            statement
                .query_map(params![project, after, upper, limit as i64 + 1], |r| {
                    r.get(0)
                })
                .map_err(storage_error)?
                .collect::<Result<_, _>>()
                .map_err(storage_error)?
        };
        let has_more = ids.len() > limit;
        let records = ids
            .iter()
            .take(limit)
            .map(|id| {
                media(&tx, project, id)?.ok_or_else(|| {
                    EditorError::new(
                        "STORAGE_ERROR",
                        "Project media disappeared within its read snapshot",
                    )
                })
            })
            .collect::<Result<_, _>>()?;
        tx.commit().map_err(storage_error)?;
        Ok(MediaPageSlice {
            records,
            upper_id: Some(upper_id),
            has_more,
        })
    }
}
