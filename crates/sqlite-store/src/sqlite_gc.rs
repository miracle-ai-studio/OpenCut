use super::*;
use opencut_editor_application::gc::*;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
pub(super) const MIGRATION:&str="
CREATE TABLE cas_gc_plans(plan_id TEXT PRIMARY KEY,idempotency_key TEXT NOT NULL UNIQUE,input_json TEXT NOT NULL,plan_json TEXT NOT NULL,created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
CREATE TABLE cas_gc_events(sequence INTEGER PRIMARY KEY AUTOINCREMENT,plan_id TEXT NOT NULL REFERENCES cas_gc_plans(plan_id),item_index INTEGER NOT NULL,state_json TEXT NOT NULL,request_id TEXT NOT NULL,actor_json TEXT NOT NULL,created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
CREATE INDEX cas_gc_events_plan_idx ON cas_gc_events(plan_id,sequence);
UPDATE metadata SET value='12' WHERE key='schema_version' AND value='11';
";
fn load(c: &Connection, id: &str) -> Result<StoredPlan, EditorError> {
    if id.len() > 128 {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Invalid collection plan ID",
        ));
    }
    let text: Option<String> = c
        .query_row(
            "SELECT plan_json FROM cas_gc_plans WHERE plan_id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage_error)?;
    let plan: StoredPlan = decode_json(
        &text.ok_or_else(|| EditorError::new("NOT_FOUND", "Collection plan not found"))?,
        "collection plan",
    )?;
    if plan.view.items.len() != plan.observations.len() || plan.observations.len() > MAX_OBJECTS {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Invalid stored collection plan",
        ));
    }
    let hash = format!(
        "{:x}",
        Sha256::digest(
            encode_json(
                &(&plan.view.plan_id, &plan.observations),
                "collection identity"
            )?
            .as_bytes()
        )
    );
    if hash != plan.view.plan_hash
        || plan
            .view
            .items
            .iter()
            .zip(&plan.observations)
            .any(|(item, obs)| item.object != obs.object || item.size_bytes != obs.size_bytes)
    {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Stored collection plan identity mismatch",
        ));
    }
    for object in &plan.observations {
        validate_object(&object.object)?;
    }
    Ok(plan)
}
fn replay(c: &Connection, input: &PlanInput) -> Result<Option<StoredPlan>, EditorError> {
    let old: Option<(String, String)> = c
        .query_row(
            "SELECT plan_id,input_json FROM cas_gc_plans WHERE idempotency_key=?1",
            [&input.idempotency_key],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(storage_error)?;
    old.map(|(id, parameters)| {
        if parameters != encode_json(input, "collection input")? {
            return Err(EditorError::new(
                "IDEMPOTENCY_KEY_REUSED",
                "Collection key belongs to different parameters",
            ));
        }
        load(c, &id)
    })
    .transpose()
}
impl ReferenceStore for EditorApplication {
    fn gc_query(&self, input: &Query) -> Result<Page, EditorError> {
        sqlite_access::owner(&self.connection.1)?;
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let after = if let Some(cursor) = &input.cursor {
            let prefix = if input.include_complete {
                "gc1:a:"
            } else {
                "gc1:u:"
            };
            let cursor = cursor.strip_prefix(prefix).ok_or_else(|| {
                EditorError::new(
                    "INVALID_CURSOR",
                    "Collection cursor belongs to a different filter or format",
                )
            })?;
            tx.query_row(
                "SELECT rowid FROM cas_gc_plans WHERE plan_id=?1",
                [cursor],
                |r| r.get::<_, i64>(0),
            )
            .optional()
            .map_err(storage_error)?
            .ok_or_else(|| {
                EditorError::new(
                    "INVALID_CURSOR",
                    "Collection cursor is absent from this workspace",
                )
            })?
        } else {
            0
        };
        let mut statement = tx
            .prepare("SELECT plan_id FROM cas_gc_plans WHERE rowid>?1 ORDER BY rowid LIMIT ?2")
            .map_err(storage_error)?;
        let mut ids = statement
            .query_map(params![after, (input.limit + 1) as i64], |r| {
                r.get::<_, String>(0)
            })
            .map_err(storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage_error)?;
        let more = ids.len() > input.limit;
        ids.truncate(input.limit);
        let examined = ids.len();
        let next_cursor = if more { ids.last().cloned() } else { None };
        let mut plans = Vec::new();
        for id in ids {
            let view = load(&tx, &id)?.view;
            if !input.include_complete && matches!(view.state, PlanState::Complete) {
                continue;
            }
            plans.push(Summary {
                plan_id: view.plan_id,
                plan_hash: view.plan_hash,
                state: view.state,
                origin_request_id: view.origin_request_id,
                actor: view.actor,
                object_count: view.items.len(),
                remaining: view
                    .items
                    .iter()
                    .filter(|i| matches!(i.state, ItemState::Planned | ItemState::Removing))
                    .count(),
            });
        }
        Ok(Page {
            plans,
            next_cursor: next_cursor.map(|id| {
                format!(
                    "gc1:{}:{id}",
                    if input.include_complete { "a" } else { "u" }
                )
            }),
            examined,
        })
    }

    fn gc_context(&self) -> opencut_editor_application::audit::ExecutionContext {
        self.connection.1.clone()
    }
    fn gc_replay(&self, input: &PlanInput) -> Result<Option<StoredPlan>, EditorError> {
        sqlite_access::owner(&self.connection.1)?;
        replay(&self.connection, input)
    }
    fn gc_load(&self, id: &str) -> Result<StoredPlan, EditorError> {
        sqlite_access::owner(&self.connection.1)?;
        load(&self.connection, id)
    }
    fn gc_protected(&self, objects: &[Observation]) -> Result<References, EditorError> {
        sqlite_access::owner(&self.connection.1)?;
        if objects.len() > MAX_OBJECTS {
            return Err(EditorError::new(
                "INVALID_ARGUMENT",
                "Too many collection objects",
            ));
        }
        let tokens: Vec<_> = objects
            .iter()
            .map(|o| {
                (
                    key(&o.object),
                    match &o.object {
                        Object::Content { content_hash } => content_hash.clone(),
                        Object::Capture { capture_id } => format!(".capture-{capture_id}"),
                    },
                )
            })
            .collect();
        let mut protected = BTreeSet::new();
        let mut native_paths = BTreeSet::new();
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let mut bytes = 0usize;
        let mut count = 0usize;
        let start = std::time::Instant::now();
        // Retain every historical revision, every accepted job (including terminal), and
        // all imported project resources. Conservative text matching also protects legacy
        // unversioned native URIs and opaque extension payloads; it may retain extra objects.
        for sql in [
            "SELECT document_json FROM revisions",
            "SELECT input_json FROM jobs",
            "SELECT result_json FROM jobs WHERE result_json IS NOT NULL",
            "SELECT asset_json FROM project_media",
        ] {
            let mut statement = tx.prepare(sql).map_err(storage_error)?;
            let mut rows = statement.query([]).map_err(storage_error)?;
            while let Some(row) = rows.next().map_err(storage_error)? {
                let document: String = row.get(0).map_err(storage_error)?;
                bytes = bytes.saturating_add(document.len());
                count += 1;
                if bytes > MAX_REFERENCE_BYTES
                    || document.len() > MAX_REFERENCE_DOCUMENT_BYTES
                    || count > MAX_REFERENCE_ROWS
                    || start.elapsed() > Duration::from_millis(MAX_REFERENCE_MS)
                {
                    return Err(EditorError::new(
                        "RESOURCE_TOO_LARGE",
                        "Collection reference scan exceeded its budget before deletion in this call",
                    ));
                }
                let value: Value = decode_json(&document, "collection reference root")?;
                // Resolve only authoritative legacy source locations, never arbitrary
                // user metadata keys. Pinned versions/manifests use their CAS hashes.
                let assets = if sql == "SELECT document_json FROM revisions" {
                    value.get("assets")
                } else if sql == "SELECT input_json FROM jobs" {
                    if let Some(Value::String(path)) = value.get("input") {
                        native_paths.insert(path.clone());
                    }
                    if value.get("asset_lock").is_none_or(Value::is_null) {
                        value
                            .get("snapshot")
                            .and_then(|s| s.get("project"))
                            .and_then(|p| p.get("assets"))
                    } else {
                        None
                    }
                } else {
                    None
                };
                if let Some(Value::Array(assets)) = assets {
                    for asset in assets {
                        if asset.get("version").is_none_or(Value::is_null)
                            && let Some(Value::String(path)) = asset.get("uri")
                            && !path.starts_with("opencut-media://")
                        {
                            native_paths.insert(path.clone());
                        }
                    }
                }
                let mut pending = vec![&value];
                let mut check = |text: &str| {
                    for (id, token) in &tokens {
                        if !protected.contains(id) && text.contains(token) {
                            protected.insert(id.clone());
                        }
                    }
                };
                while let Some(value) = pending.pop() {
                    match value {
                        Value::String(text) => check(text),
                        Value::Array(values) => pending.extend(values),
                        Value::Object(values) => {
                            for (key, value) in values {
                                check(key);
                                pending.push(value);
                            }
                        }
                        _ => {}
                    }
                }
                if start.elapsed() > Duration::from_millis(MAX_REFERENCE_MS) {
                    return Err(EditorError::new(
                        "RESOURCE_TOO_LARGE",
                        "Collection reference scan exceeded its budget before deletion in this call",
                    ));
                }
            }
        }
        Ok(References {
            objects: protected,
            native_paths,
        })
    }
    fn gc_create(
        &mut self,
        input: &PlanInput,
        observed: Vec<Observation>,
        protected: BTreeSet<String>,
        next: Option<String>,
    ) -> Result<StoredPlan, EditorError> {
        let ctx = self.connection.1.clone();
        sqlite_access::owner(&ctx)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        if let Some(old) = replay(&tx, input)? {
            return Ok(old);
        }
        let examined = observed.len();
        let observations: Vec<_> = observed
            .into_iter()
            .filter(|o| !protected.contains(&key(&o.object)))
            .collect();
        let id = format!("gc_{}", Uuid::new_v4().simple());
        let hash = format!(
            "{:x}",
            Sha256::digest(encode_json(&(&id, &observations), "collection identity")?.as_bytes())
        );
        let plan = StoredPlan {
            view: Plan {
                plan_id: id,
                plan_hash: hash,
                state: if observations.is_empty() {
                    PlanState::Complete
                } else {
                    PlanState::Planned
                },
                origin_request_id: ctx.request_id().into(),
                actor: ctx.actor().clone(),
                examined,
                protected: examined - observations.len(),
                next_cursor: next,
                items: observations
                    .iter()
                    .map(|o| Item {
                        object: o.object.clone(),
                        size_bytes: o.size_bytes.clone(),
                        state: ItemState::Planned,
                        request_id: None,
                        actor: None,
                    })
                    .collect(),
            },
            observations,
        };
        tx.execute("INSERT INTO cas_gc_plans(plan_id,idempotency_key,input_json,plan_json) VALUES (?1,?2,?3,?4)",params![plan.view.plan_id,input.idempotency_key,encode_json(input,"collection input")?,encode_json(&plan,"collection plan")?]).map_err(storage_error)?;
        sqlite_audit::insert(&tx, &ctx, "cas.gc.planned", sqlite_audit::Links::default())?;
        tx.commit().map_err(storage_error)?;
        Ok(plan)
    }
    fn gc_mark(&mut self, id: &str, index: usize, state: ItemState) -> Result<(), EditorError> {
        let ctx = self.connection.1.clone();
        sqlite_access::owner(&ctx)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let mut plan = load(&tx, id)?;
        let item = plan
            .view
            .items
            .get_mut(index)
            .ok_or_else(|| EditorError::new("STORAGE_ERROR", "Invalid collection item"))?;
        if !matches!(item.state, ItemState::Planned | ItemState::Removing) {
            return Ok(());
        }
        item.state = state.clone();
        item.request_id = Some(ctx.request_id().into());
        item.actor = Some(ctx.actor().clone());
        plan.view.state = if plan
            .view
            .items
            .iter()
            .any(|i| matches!(i.state, ItemState::Planned | ItemState::Removing))
        {
            PlanState::Executing
        } else {
            PlanState::Complete
        };
        tx.execute(
            "UPDATE cas_gc_plans SET plan_json=?1 WHERE plan_id=?2",
            params![encode_json(&plan, "collection plan")?, id],
        )
        .map_err(storage_error)?;
        tx.execute("INSERT INTO cas_gc_events(plan_id,item_index,state_json,request_id,actor_json) VALUES (?1,?2,?3,?4,?5)",params![id,index as i64,encode_json(&state,"collection state")?,ctx.request_id(),encode_json(ctx.actor(),"collection actor")?]).map_err(storage_error)?;
        sqlite_audit::insert(
            &tx,
            &ctx,
            "cas.gc.item",
            sqlite_audit::Links {
                origin_request_id: Some(&plan.view.origin_request_id),
                ..Default::default()
            },
        )?;
        tx.commit().map_err(storage_error)
    }
}
