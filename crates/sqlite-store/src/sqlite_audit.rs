//! Native persistence of Application provenance. Called inside business transactions.
use super::*;
use opencut_api_contracts::audit::*;
use opencut_editor_application::audit::{self, AuditRead, ExecutionContext};

pub(super) const MIGRATION: &str = "
CREATE TABLE audit_records (
 sequence INTEGER PRIMARY KEY AUTOINCREMENT,
 audit_id TEXT NOT NULL UNIQUE,
 request_id TEXT NOT NULL,
 origin_request_id TEXT,
 actor_json TEXT NOT NULL,
 action TEXT NOT NULL,
 project_id TEXT REFERENCES projects(project_id),
 revision TEXT REFERENCES revisions(revision),
 transaction_id TEXT REFERENCES editor_transactions(transaction_id),
 job_id TEXT REFERENCES jobs(job_id),
 attempt INTEGER,
 artifact_ids_json TEXT NOT NULL,
 created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX audit_project_idx ON audit_records(project_id, sequence);
CREATE INDEX audit_job_idx ON audit_records(job_id, sequence);
CREATE INDEX audit_transaction_idx ON audit_records(transaction_id, sequence);
CREATE INDEX audit_request_idx ON audit_records(request_id, sequence);
CREATE INDEX audit_origin_idx ON audit_records(origin_request_id, sequence);
UPDATE metadata SET value='4' WHERE key='schema_version' AND value='3';
";

#[derive(Default)]
pub(super) struct Links<'a> {
    pub origin_request_id: Option<&'a str>,
    pub project_id: Option<&'a str>,
    pub revision: Option<&'a str>,
    pub transaction_id: Option<&'a str>,
    pub job_id: Option<&'a str>,
    pub attempt: Option<u32>,
    pub artifact_ids: Vec<String>,
}
pub(super) fn insert(
    tx: &Connection,
    context: &ExecutionContext,
    action: &str,
    links: Links<'_>,
) -> Result<(), EditorError> {
    tx.execute("INSERT INTO audit_records(audit_id,request_id,origin_request_id,actor_json,action,project_id,revision,transaction_id,job_id,attempt,artifact_ids_json) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
      params![format!("audit_{}",Uuid::new_v4().simple()),context.request_id(),links.origin_request_id,encode_json(context.actor(),"audit actor")?,action,links.project_id,links.revision,links.transaction_id,links.job_id,links.attempt,encode_json(&links.artifact_ids,"audit artifacts")?])
      .map_err(storage_error)?;
    Ok(())
}
pub(super) fn job(
    tx: &Connection,
    context: &ExecutionContext,
    record: &JobRecord,
    action: &str,
    accepted: bool,
) -> Result<(), EditorError> {
    let origin: Option<String> = if accepted {
        Some(context.request_id().into())
    } else {
        tx.query_row("SELECT request_id FROM audit_records WHERE job_id=?1 AND action='job.accepted' ORDER BY sequence LIMIT 1",[&record.job_id],|r|r.get(0)).optional().map_err(storage_error)?
    };
    let view = opencut_editor_application::job_views::project(record.clone());
    let target: Option<(String, String)> = tx
        .query_row(
            "SELECT project_id,revision FROM job_projects WHERE job_id=?1",
            [&record.job_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(storage_error)?;
    insert(
        tx,
        context,
        action,
        Links {
            origin_request_id: origin.as_deref(),
            project_id: target.as_ref().map(|t| t.0.as_str()),
            revision: target.as_ref().map(|t| t.1.as_str()),
            job_id: Some(&record.job_id),
            attempt: Some(record.attempt),
            artifact_ids: view.artifacts.into_iter().map(|a| a.artifact_id).collect(),
            ..Default::default()
        },
    )
}
impl AuditRead for SqliteConnection {
    fn read_audit(
        &self,
        target: &AuditTarget,
        after: i64,
        limit: usize,
    ) -> Result<Vec<AuditRecord>, EditorError> {
        let tx = self.unchecked_transaction().map_err(storage_error)?;
        let principal = sqlite_access::principal(&tx, &self.1)?;
        if principal.is_some() {
            match target {
                AuditTarget::Project(id) => {
                    sqlite_access::require(&tx, &self.1, id, ProjectAction::ReadAudit)?
                }
                AuditTarget::Job(id) => {
                    sqlite_access::require_job(&tx, &self.1, id, ProjectAction::ReadAudit)?
                }
                AuditTarget::Transaction(id) => {
                    let project: Option<String> = tx
                        .query_row(
                            "SELECT project_id FROM editor_transactions WHERE transaction_id=?1",
                            [id],
                            |r| r.get(0),
                        )
                        .optional()
                        .map_err(storage_error)?;
                    sqlite_access::require(
                        &tx,
                        &self.1,
                        &project.ok_or_else(access::denied)?,
                        ProjectAction::ReadAudit,
                    )?;
                }
                _ => {}
            }
        }
        let (filter, id) = match target {
            AuditTarget::Workspace => ("1=1", ""),
            AuditTarget::Project(id) => ("project_id=?2", id.as_str()),
            AuditTarget::Transaction(id) => ("transaction_id=?2", id.as_str()),
            AuditTarget::Job(id) => ("job_id=?2", id.as_str()),
            AuditTarget::Request(id) => ("(request_id=?2 OR origin_request_id=?2)", id.as_str()),
        };
        let mut statement=tx.prepare(&format!("SELECT sequence,audit_id,request_id,origin_request_id,actor_json,action,project_id,revision,transaction_id,job_id,attempt,artifact_ids_json,created_at FROM audit_records WHERE sequence>?1 AND {filter} AND ?2 IS NOT NULL AND (?4 IS NULL OR EXISTS(SELECT 1 FROM access_grants g WHERE g.principal_id=?4 AND g.project_id=audit_records.project_id AND g.action_mask & 64 != 0 AND (audit_records.job_id IS NULL OR EXISTS(SELECT 1 FROM job_projects jp WHERE jp.job_id=audit_records.job_id AND jp.project_id=audit_records.project_id)))) ORDER BY sequence LIMIT ?3")).map_err(storage_error)?;
        let rows = statement
            .query_map(params![after, id, limit as i64, principal], |r| {
                Ok((
                    AuditRecord {
                        sequence: r.get::<_, i64>(0)?.to_string(),
                        audit_id: r.get(1)?,
                        request_id: r.get(2)?,
                        origin_request_id: r.get(3)?,
                        actor: Actor {
                            principal_id: String::new(),
                            source: CallSource::Embedded,
                            worker_id: None,
                        },
                        action: r.get(5)?,
                        project_id: r.get(6)?,
                        revision: r.get(7)?,
                        transaction_id: r.get(8)?,
                        job_id: r.get(9)?,
                        attempt: r.get(10)?,
                        artifact_ids: vec![],
                        created_at: r.get(12)?,
                    },
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(11)?,
                ))
            })
            .map_err(storage_error)?;
        rows.map(|row| {
            let (mut record, actor, artifacts) = row.map_err(storage_error)?;
            record.actor = decode_json(&actor, "stored audit actor")?;
            record.artifact_ids = decode_json(&artifacts, "stored audit artifacts")?;
            Ok(record)
        })
        .collect()
    }
}
impl EditorApplication {
    pub fn execution_context(&self) -> &ExecutionContext {
        &self.connection.1
    }
    pub fn set_execution_context(&mut self, context: ExecutionContext) {
        self.connection.1 = context;
    }
    pub fn audit_query(&self, input: AuditQuery) -> Result<AuditPage, EditorError> {
        audit::query(&self.connection, input)
    }
}
