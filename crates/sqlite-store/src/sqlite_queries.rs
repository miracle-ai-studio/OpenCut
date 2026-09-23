//! Native read adapters. Multi-row pages use a single SQLite read snapshot.
use super::*;
use queries::{EventAnchor, JobPageSlice, JobRead, WorkspaceRead};
impl JobRead for SqliteConnection {
    fn read_job(&self, id: &str) -> Result<JobRecord, EditorError> {
        sqlite_access::require_job(self, &self.1, id, ProjectAction::Read)?;
        load_job(self, id)
    }
    fn read_jobs(&self) -> Result<Vec<JobRecord>, EditorError> {
        let principal = sqlite_access::principal(self, &self.1)?;
        let mut statement = self
            .prepare("SELECT job_id FROM jobs WHERE (?1 IS NULL OR EXISTS(SELECT 1 FROM job_projects jp JOIN access_grants g USING(project_id) WHERE jp.job_id=jobs.job_id AND g.principal_id=?1 AND g.action_mask & 1 != 0)) ORDER BY created_at, job_id")
            .map_err(storage_error)?;
        let ids = statement
            .query_map([principal], |row| row.get::<_, String>(0))
            .map_err(storage_error)?;
        ids.map(|id| load_job(self, &id.map_err(storage_error)?))
            .collect()
    }

    fn read_job_page(
        &self,
        upper: Option<&str>,
        after: Option<&str>,
        limit: usize,
    ) -> Result<JobPageSlice, EditorError> {
        let tx = self.unchecked_transaction().map_err(storage_error)?;
        let principal = sqlite_access::principal(&tx, &self.1)?;
        let upper = if let Some(upper) = upper {
            Some(upper.to_owned())
        } else {
            tx.query_row(
                "SELECT job_id FROM jobs WHERE (?1 IS NULL OR EXISTS(SELECT 1 FROM job_projects jp JOIN access_grants g USING(project_id) WHERE jp.job_id=jobs.job_id AND g.principal_id=?1 AND g.action_mask & 1 != 0)) ORDER BY rowid DESC LIMIT 1",
                [&principal],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage_error)?
        };
        let Some(upper_id) = upper else {
            return Ok(JobPageSlice {
                records: vec![],
                upper_id: None,
                has_more: false,
            });
        };
        let rowid = |id: &str| -> Result<i64, EditorError> {
            tx.query_row("SELECT rowid FROM jobs WHERE job_id=?1 AND (?2 IS NULL OR EXISTS(SELECT 1 FROM job_projects jp JOIN access_grants g USING(project_id) WHERE jp.job_id=jobs.job_id AND g.principal_id=?2 AND g.action_mask & 1 != 0))", params![id,principal], |r| r.get(0))
                .optional()
                .map_err(storage_error)?
                .ok_or_else(|| {
                    EditorError::new("CURSOR_EXPIRED", "Task page anchor is unavailable")
                })
        };
        let upper = rowid(&upper_id)?;
        let after = after.map(rowid).transpose()?.unwrap_or(0);
        if after > upper {
            return Err(EditorError::new(
                "INVALID_CURSOR",
                "Invalid task page interval",
            ));
        }
        let ids: Vec<String> = {
            let mut statement = tx
                .prepare(
                    "SELECT job_id FROM jobs WHERE rowid>?1 AND rowid<=?2 AND (?4 IS NULL OR EXISTS(SELECT 1 FROM job_projects jp JOIN access_grants g USING(project_id) WHERE jp.job_id=jobs.job_id AND g.principal_id=?4 AND g.action_mask & 1 != 0)) ORDER BY rowid LIMIT ?3",
                )
                .map_err(storage_error)?;
            statement
                .query_map(params![after, upper, limit as i64 + 1, principal], |r| {
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
            .map(|id| load_job(&tx, id))
            .collect::<Result<_, _>>()?;
        tx.commit().map_err(storage_error)?;
        Ok(JobPageSlice {
            records,
            upper_id: Some(upper_id),
            has_more,
        })
    }
    fn read_job_changes(
        &self,
        anchor: Option<&EventAnchor>,
        job_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<JobEvent>, EditorError> {
        let tx = self.unchecked_transaction().map_err(storage_error)?;
        let principal = sqlite_access::principal(&tx, &self.1)?;
        if let Some(id) = job_id {
            sqlite_access::require_job(&tx, &self.1, id, ProjectAction::Read)?;
            load_job(&tx, id)?;
        }
        let after = if let Some(anchor) = anchor {
            let row: Option<(String, String)> = tx
                .query_row(
                    "SELECT event_id,job_id FROM job_events WHERE sequence=?1 AND (?2 IS NULL OR EXISTS(SELECT 1 FROM job_projects jp JOIN access_grants g USING(project_id) WHERE jp.job_id=job_events.job_id AND g.principal_id=?2 AND g.action_mask & 1 != 0))",
                    params![anchor.sequence,principal],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(storage_error)?;
            if !row.is_some_and(|(id, job)| {
                id == anchor.event_id && job_id.is_none_or(|wanted| wanted == job)
            }) {
                return Err(EditorError::new(
                    "CURSOR_EXPIRED",
                    "Event cursor anchor is unavailable",
                ));
            }
            anchor.sequence
        } else {
            0
        };
        let events = read_job_events(&tx, after, limit, job_id, principal.as_deref())?;
        tx.commit().map_err(storage_error)?;
        Ok(events)
    }
    fn read_job_events(
        &self,
        after: i64,
        limit: usize,
        job_id: Option<&str>,
    ) -> Result<Vec<JobEvent>, EditorError> {
        if let Some(id) = job_id {
            sqlite_access::require_job(self, &self.1, id, ProjectAction::Read)?;
            load_job(self, id)?;
        }
        let principal = sqlite_access::principal(self, &self.1)?;
        read_job_events(self, after, limit, job_id, principal.as_deref())
    }
}
impl WorkspaceRead for SqliteConnection {
    fn read_projects(&self) -> Result<Vec<ProjectSnapshot>, EditorError> {
        let principal = sqlite_access::principal(self, &self.1)?;
        let mut statement = self
            .prepare(
                "SELECT r.document_json, p.head_revision
                 FROM projects p JOIN revisions r ON r.revision = p.head_revision
                 WHERE (?1 IS NULL OR EXISTS(SELECT 1 FROM access_grants g WHERE g.principal_id=?1 AND g.project_id=p.project_id AND g.action_mask & 1 != 0))
                 ORDER BY p.project_id",
            )
            .map_err(storage_error)?;
        let rows = statement
            .query_map([principal], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(storage_error)?;
        rows.map(|row| {
            let (document, revision) = row.map_err(storage_error)?;
            Ok(ProjectSnapshot {
                project: decode_json(&document, "stored project document")?,
                revision,
            })
        })
        .collect()
    }

    fn read_revision_summary(
        &self,
        project_id: &str,
        revision: &str,
    ) -> Result<RevisionSummary, EditorError> {
        sqlite_access::require(self, &self.1, project_id, ProjectAction::Read)?;
        let row:Option<HistoryRow>=self.query_row(
            "SELECT r.revision,r.parent_revision,r.transaction_id,r.created_at,t.request_json FROM revisions r LEFT JOIN editor_transactions t ON t.transaction_id=r.transaction_id WHERE r.project_id=?1 AND r.revision=?2",
            params![project_id,revision],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional().map_err(storage_error)?;
        let (revision, parent_revision, transaction_id, created_at, request) =
            row.ok_or_else(|| {
                EditorError::new(
                    "NOT_FOUND",
                    "History cursor does not belong to this project",
                )
            })?;
        let label = request
            .map(|s| decode_json::<Value>(&s, "stored transaction request"))
            .transpose()?
            .and_then(|v| v.get("label").and_then(Value::as_str).map(str::to_owned));
        Ok(RevisionSummary {
            revision,
            parent_revision,
            transaction_id,
            created_at,
            label,
        })
    }
    fn read_project_events(
        &self,
        after_sequence: i64,
        limit: usize,
    ) -> Result<Vec<StoredEvent>, EditorError> {
        let principal = sqlite_access::principal(self, &self.1)?;
        let mut statement = self
            .prepare(
                "SELECT sequence, event_id, project_id, revision, event_type, payload_json
             FROM outbox WHERE sequence > ?1 AND (?3 IS NULL OR EXISTS(SELECT 1 FROM access_grants g WHERE g.principal_id=?3 AND g.project_id=outbox.project_id AND g.action_mask & 1 != 0)) ORDER BY sequence LIMIT ?2",
            )
            .map_err(storage_error)?;
        let rows = statement
            .query_map(params![after_sequence, limit as i64, principal], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get::<_, String>(5)?,
                ))
            })
            .map_err(storage_error)?;
        rows.map(|row| {
            let (sequence, event_id, project_id, revision, event_type, payload) =
                row.map_err(storage_error)?;
            Ok(StoredEvent {
                sequence,
                event_id,
                project_id,
                revision,
                event_type,
                payload: decode_json(&payload, "stored event")?,
            })
        })
        .collect()
    }
}
fn read_job_events(
    connection: &Connection,
    after_sequence: i64,
    limit: usize,
    job_id: Option<&str>,
    principal: Option<&str>,
) -> Result<Vec<JobEvent>, EditorError> {
    let mut statement = connection
        .prepare(
            "SELECT sequence,event_id,job_id,event_type,state,attempt,payload_json,created_at
                 FROM job_events
                 WHERE sequence > ?1 AND (?2 IS NULL OR job_id = ?2) AND (?4 IS NULL OR EXISTS(SELECT 1 FROM job_projects jp JOIN access_grants g USING(project_id) WHERE jp.job_id=job_events.job_id AND g.principal_id=?4 AND g.action_mask & 1 != 0))
                 ORDER BY sequence LIMIT ?3",
        )
        .map_err(storage_error)?;
    let rows = statement
        .query_map(
            params![after_sequence, job_id, limit as i64, principal],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, u32>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            },
        )
        .map_err(storage_error)?;
    rows.map(|row| {
        let row = row.map_err(storage_error)?;
        Ok(JobEvent {
            sequence: row.0,
            event_id: row.1,
            job_id: row.2,
            event_type: row.3,
            state: parse_job_state(&row.4)?,
            attempt: row.5,
            payload: decode_json(&row.6, "job event payload")?,
            created_at: row.7,
        })
    })
    .collect()
}

impl opencut_editor_application::events::ProjectEventRead for SqliteConnection {
    fn read_project_changes(
        &self,
        anchor: Option<&EventAnchor>,
        project_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<StoredEvent>, EditorError> {
        let tx = self.unchecked_transaction().map_err(storage_error)?;
        let principal = sqlite_access::principal(&tx, &self.1)?;
        if let Some(id) = project_id {
            sqlite_access::require(&tx, &self.1, id, ProjectAction::Read)?;
            load_snapshot(&tx, id)?;
        }
        let after = if let Some(anchor) = anchor {
            let row: Option<(String, String)> = tx
                .query_row(
                    "SELECT event_id,project_id FROM outbox WHERE sequence=?1 AND (?2 IS NULL OR EXISTS(SELECT 1 FROM access_grants g WHERE g.principal_id=?2 AND g.project_id=outbox.project_id AND g.action_mask & 1 != 0))",
                    params![anchor.sequence,principal],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(storage_error)?;
            if !row.is_some_and(|(id, project)| {
                id == anchor.event_id && project_id.is_none_or(|wanted| wanted == project)
            }) {
                return Err(EditorError::new(
                    "CURSOR_EXPIRED",
                    "Event cursor anchor is unavailable",
                ));
            }
            anchor.sequence
        } else {
            0
        };
        let events = {
            let mut statement=tx.prepare("SELECT sequence,event_id,project_id,revision,event_type,payload_json FROM outbox WHERE sequence>?1 AND (?2 IS NULL OR project_id=?2) AND (?4 IS NULL OR EXISTS(SELECT 1 FROM access_grants g WHERE g.principal_id=?4 AND g.project_id=outbox.project_id AND g.action_mask & 1 != 0)) ORDER BY sequence LIMIT ?3").map_err(storage_error)?;
            statement
                .query_map(params![after, project_id, limit as i64, principal], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                    ))
                })
                .map_err(storage_error)?
                .map(|row| {
                    let (sequence, event_id, project_id, revision, event_type, payload) =
                        row.map_err(storage_error)?;
                    Ok(StoredEvent {
                        sequence,
                        event_id,
                        project_id,
                        revision,
                        event_type,
                        payload: decode_json(&payload, "stored project event")?,
                    })
                })
                .collect::<Result<Vec<_>, EditorError>>()?
        };
        tx.commit().map_err(storage_error)?;
        Ok(events)
    }
}
