//! Durable, owner-only artifact collection review plans. No deletion entry exists yet.
use super::*;
use opencut_api_contracts::{
    artifact_gc::{Page, Plan, PlanInput, Query, Summary},
    artifact_retention::ReferenceRoot,
};
use opencut_editor_application::artifact_gc::{Observation, Snapshot, Source, Store};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub(super) const MIGRATION: &str = "
CREATE TABLE artifact_gc_plans(
  plan_id TEXT PRIMARY KEY,
  idempotency_key TEXT NOT NULL UNIQUE,
  input_json TEXT NOT NULL,
  plan_json TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
UPDATE metadata SET value='17' WHERE key='schema_version' AND value='16';
";

#[derive(Serialize, Deserialize)]
struct StoredPlan {
    view: Plan,
    observation: Observation,
    all_roots: BTreeSet<ReferenceRoot>,
    alias_roots: BTreeSet<ReferenceRoot>,
}

fn identity_hash(stored: &StoredPlan) -> Result<String, EditorError> {
    let mut view = stored.view.clone();
    view.plan_hash.clear();
    Ok(format!(
        "{:x}",
        Sha256::digest(
            encode_json(
                &(
                    &view,
                    &stored.observation,
                    &stored.all_roots,
                    &stored.alias_roots,
                ),
                "artifact plan identity"
            )?
            .as_bytes()
        )
    ))
}

fn load(connection: &Connection, plan_id: &str) -> Result<Plan, EditorError> {
    let text: Option<String> = connection
        .query_row(
            "SELECT plan_json FROM artifact_gc_plans WHERE plan_id=?1",
            [plan_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage_error)?;
    let stored: StoredPlan = decode_json(
        &text.ok_or_else(|| EditorError::new("NOT_FOUND", "Artifact plan not found"))?,
        "artifact plan",
    )?;
    if stored.view.plan_id != plan_id
        || stored.view.plan_hash != identity_hash(&stored)?
        || stored.view.root_count != stored.all_roots.len().to_string()
        || stored.view.roots
            != stored
                .all_roots
                .iter()
                .take(100)
                .cloned()
                .collect::<Vec<_>>()
        || stored.view.roots_truncated != (stored.all_roots.len() > 100)
        || stored.view.native_alias_root_count != stored.alias_roots.len().to_string()
        || stored.view.size_bytes != stored.observation.size_bytes
        || stored.view.execution_available
    {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Artifact plan identity is invalid",
        ));
    }
    Ok(stored.view)
}

fn replay(connection: &Connection, input: &PlanInput) -> Result<Option<Plan>, EditorError> {
    let row: Option<(String, String)> = connection
        .query_row(
            "SELECT plan_id,input_json FROM artifact_gc_plans WHERE idempotency_key=?1",
            [&input.idempotency_key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(storage_error)?;
    row.map(|(plan_id, prior)| {
        if prior != encode_json(input, "artifact plan input")? {
            return Err(EditorError::new(
                "IDEMPOTENCY_KEY_REUSED",
                "Artifact plan key belongs to different parameters",
            ));
        }
        load(connection, &plan_id)
    })
    .transpose()
}

impl Store for EditorApplication {
    fn artifact_plan_context(&self) -> opencut_editor_application::audit::ExecutionContext {
        self.connection.1.clone()
    }

    fn artifact_plan_replay(&self, input: &PlanInput) -> Result<Option<Plan>, EditorError> {
        sqlite_access::owner(&self.connection.1)?;
        replay(&self.connection, input)
    }

    fn artifact_plan_snapshot(&self, artifact_id: &str) -> Result<Snapshot, EditorError> {
        sqlite_access::owner(&self.connection.1)?;
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let (job, _) = sqlite_artifact_retention::published(&tx, artifact_id)?;
        let path = job
            .result
            .as_ref()
            .and_then(|result| result.pointer("/artifact/path"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                EditorError::new("INVALID_STORED_RESULT", "Published artifact path is absent")
            })?;
        let references = sqlite_artifact_references::scan(&tx, &job.job_id, artifact_id, path)?;
        let default_retained: bool = tx
            .query_row(
                "SELECT NOT EXISTS(SELECT 1 FROM artifact_retention WHERE artifact_id=?1)",
                [artifact_id],
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        Ok(Snapshot {
            source: Source {
                job_id: job.job_id,
                attempt: job.attempt,
                path: path.into(),
            },
            default_retained,
            roots: references.roots,
            native_sources: references.native_paths,
            examined_rows: references.examined_rows,
        })
    }

    fn artifact_plan_create(
        &mut self,
        input: &PlanInput,
        observation: Observation,
        snapshot: Snapshot,
        alias_roots: BTreeSet<ReferenceRoot>,
    ) -> Result<Plan, EditorError> {
        let context = self.connection.1.clone();
        sqlite_access::owner(&context)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        if let Some(previous) = replay(&tx, input)? {
            return Ok(previous);
        }
        let mut all_roots = snapshot.roots;
        all_roots.extend(alias_roots.iter().cloned());
        let observed_at: String = tx
            .query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ','now')", [], |row| {
                row.get(0)
            })
            .map_err(storage_error)?;
        let view = Plan {
            plan_id: format!("agc_{}", Uuid::new_v4().simple()),
            plan_hash: String::new(),
            artifact_id: input.artifact_id.clone(),
            size_bytes: observation.size_bytes.clone(),
            observed_at,
            default_retained: snapshot.default_retained,
            roots: all_roots.iter().take(100).cloned().collect(),
            root_count: all_roots.len().to_string(),
            roots_truncated: all_roots.len() > 100,
            native_alias_root_count: alias_roots.len().to_string(),
            examined_rows: snapshot.examined_rows.to_string(),
            native_paths_examined: snapshot.native_sources.len().to_string(),
            origin_request_id: context.request_id().into(),
            actor: context.actor().clone(),
            execution_available: false,
        };
        let mut stored = StoredPlan {
            view,
            observation,
            all_roots,
            alias_roots,
        };
        stored.view.plan_hash = identity_hash(&stored)?;
        tx.execute(
            "INSERT INTO artifact_gc_plans(plan_id,idempotency_key,input_json,plan_json) VALUES (?1,?2,?3,?4)",
            params![
                stored.view.plan_id,
                input.idempotency_key,
                encode_json(input, "artifact plan input")?,
                encode_json(&stored, "artifact plan")?
            ],
        )
        .map_err(storage_error)?;
        sqlite_audit::insert(
            &tx,
            &context,
            "artifact.gc.planned",
            sqlite_audit::Links::default(),
        )?;
        tx.commit().map_err(storage_error)?;
        Ok(stored.view)
    }

    fn artifact_plan_load(&self, plan_id: &str) -> Result<Plan, EditorError> {
        sqlite_access::owner(&self.connection.1)?;
        load(&self.connection, plan_id)
    }

    fn artifact_plan_query(&self, input: &Query) -> Result<Page, EditorError> {
        sqlite_access::owner(&self.connection.1)?;
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let after = if let Some(cursor) = &input.cursor {
            let plan_id = cursor.strip_prefix("agc1:").ok_or_else(|| {
                EditorError::new("INVALID_CURSOR", "Artifact plan cursor has another format")
            })?;
            tx.query_row(
                "SELECT rowid FROM artifact_gc_plans WHERE plan_id=?1",
                [plan_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(storage_error)?
            .ok_or_else(|| EditorError::new("INVALID_CURSOR", "Artifact plan cursor is absent"))?
        } else {
            0
        };
        let mut statement = tx
            .prepare("SELECT plan_id FROM artifact_gc_plans WHERE rowid>?1 ORDER BY rowid LIMIT ?2")
            .map_err(storage_error)?;
        let mut ids = statement
            .query_map(params![after, (input.limit + 1) as i64], |row| {
                row.get::<_, String>(0)
            })
            .map_err(storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage_error)?;
        let more = ids.len() > input.limit;
        ids.truncate(input.limit);
        let next_cursor = if more {
            ids.last().map(|id| format!("agc1:{id}"))
        } else {
            None
        };
        let examined = ids.len();
        let plans = ids
            .into_iter()
            .map(|id| {
                let plan = load(&tx, &id)?;
                Ok(Summary {
                    plan_id: plan.plan_id,
                    plan_hash: plan.plan_hash,
                    artifact_id: plan.artifact_id,
                    observed_at: plan.observed_at,
                    default_retained: plan.default_retained,
                    root_count: plan.root_count,
                    origin_request_id: plan.origin_request_id,
                    actor: plan.actor,
                })
            })
            .collect::<Result<Vec<_>, EditorError>>()?;
        Ok(Page {
            plans,
            next_cursor,
            examined,
        })
    }
}
