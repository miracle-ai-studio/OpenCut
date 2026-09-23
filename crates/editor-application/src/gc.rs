//! Collection orchestration. A native exclusive gate spans fresh reference checks,
//! short journal commits and durable deletion; no database write transaction spans I/O.
use crate::{access::denied, audit::ExecutionContext};
pub use opencut_api_contracts::gc::*;
use opencut_contracts::EditorError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub object: Object,
    pub size_bytes: String,
    pub identity: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredPlan {
    pub view: Plan,
    pub observations: Vec<Observation>,
}
pub struct References {
    pub objects: BTreeSet<String>,
    pub native_paths: BTreeSet<String>,
}
pub trait ReferenceStore {
    fn gc_query(&self, input: &Query) -> Result<Page, EditorError>;
    fn gc_context(&self) -> ExecutionContext;
    fn gc_replay(&self, input: &PlanInput) -> Result<Option<StoredPlan>, EditorError>;
    fn gc_protected(&self, objects: &[Observation]) -> Result<References, EditorError>;
    fn gc_create(
        &mut self,
        input: &PlanInput,
        observed: Vec<Observation>,
        protected: BTreeSet<String>,
        next: Option<String>,
    ) -> Result<StoredPlan, EditorError>;
    fn gc_load(&self, id: &str) -> Result<StoredPlan, EditorError>;
    fn gc_mark(&mut self, id: &str, index: usize, state: ItemState) -> Result<(), EditorError>;
}
pub trait Files {
    fn protect_sources(
        &self,
        objects: &[Observation],
        paths: &BTreeSet<String>,
    ) -> Result<BTreeSet<String>, EditorError>;
    fn list(&self, input: &PlanInput) -> Result<(Vec<Observation>, Option<String>), EditorError>;
    fn remove(&self, object: &Observation) -> Result<ItemState, EditorError>;
}
pub trait Native {
    type Locked: Files;
    fn exclusive(&self) -> Result<Self::Locked, EditorError>;
}
pub fn key(object: &Object) -> String {
    match object {
        Object::Content { content_hash } => format!("content:{content_hash}"),
        Object::Capture { capture_id } => format!("capture:{capture_id}"),
    }
}
pub fn valid_hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub fn validate_object(object: &Object) -> Result<(), EditorError> {
    let valid = match object {
        Object::Content { content_hash } => valid_hash(content_hash),
        Object::Capture { capture_id } => {
            uuid::Uuid::parse_str(capture_id).is_ok_and(|u| u.to_string() == *capture_id)
        }
    };
    if valid { Ok(()) } else { Err(invalid()) }
}
fn invalid() -> EditorError {
    EditorError::new("INVALID_ARGUMENT", "Invalid CAS collection request")
}
fn owner(store: &impl ReferenceStore) -> Result<(), EditorError> {
    if store.gc_context().caller().is_owner() {
        Ok(())
    } else {
        Err(denied())
    }
}
pub fn validate(input: &PlanInput) -> Result<(), EditorError> {
    if input.idempotency_key.is_empty()
        || input.idempotency_key.len() > 1024
        || input.idempotency_key.chars().any(char::is_control)
        || !(1..=MAX_OBJECTS).contains(&input.limit)
    {
        return Err(invalid());
    }
    if let Some(after) = &input.after {
        let obj = if let Some(h) = after.strip_prefix("content:") {
            Object::Content {
                content_hash: h.into(),
            }
        } else if let Some(id) = after.strip_prefix("capture:") {
            Object::Capture {
                capture_id: id.into(),
            }
        } else {
            return Err(invalid());
        };
        validate_object(&obj)?;
    }
    Ok(())
}
pub fn plan(
    store: &mut impl ReferenceStore,
    native: &impl Native,
    input: PlanInput,
) -> Result<Plan, EditorError> {
    owner(store)?;
    validate(&input)?;
    if let Some(old) = store.gc_replay(&input)? {
        return Ok(old.view);
    }
    let files = native.exclusive()?;
    if let Some(old) = store.gc_replay(&input)? {
        return Ok(old.view);
    }
    let (objects, next) = files.list(&input)?;
    let references = store.gc_protected(&objects)?;
    let mut protected = references.objects;
    protected.extend(files.protect_sources(&objects, &references.native_paths)?);
    Ok(store.gc_create(&input, objects, protected, next)?.view)
}
pub fn inspect(store: &impl ReferenceStore, input: PlanId) -> Result<Plan, EditorError> {
    owner(store)?;
    Ok(store.gc_load(&input.plan_id)?.view)
}
pub fn execute(
    store: &mut impl ReferenceStore,
    native: &impl Native,
    input: ExecuteInput,
) -> Result<Plan, EditorError> {
    owner(store)?;
    let initial = store.gc_load(&input.plan_id)?;
    if initial.view.plan_hash != input.expected_plan_hash {
        return Err(EditorError::new(
            "GC_PLAN_CONFLICT",
            "Use the exact plan hash returned by plan or inspect",
        ));
    }
    if matches!(initial.view.state, PlanState::Complete) {
        return Ok(initial.view);
    }
    let files = native.exclusive()?;
    let current = store.gc_load(&input.plan_id)?;
    if current.view.plan_hash != input.expected_plan_hash {
        return Err(EditorError::new(
            "GC_PLAN_CONFLICT",
            "Collection plan changed before execution",
        ));
    }
    if matches!(current.view.state, PlanState::Complete) {
        return Ok(current.view);
    }
    // Recheck the entire retained reference authority after acquiring the publication gate.
    let references = store.gc_protected(&current.observations)?;
    let mut protected = references.objects;
    protected.extend(files.protect_sources(&current.observations, &references.native_paths)?);
    for (index, observation) in current.observations.iter().enumerate() {
        if !matches!(
            current.view.items[index].state,
            ItemState::Planned | ItemState::Removing
        ) {
            continue;
        }
        if protected.contains(&key(&observation.object)) {
            store.gc_mark(&input.plan_id, index, ItemState::Retained)?;
            continue;
        }
        store.gc_mark(&input.plan_id, index, ItemState::Removing)?;
        let state = files.remove(observation)?;
        store.gc_mark(&input.plan_id, index, state)?;
    }
    Ok(store.gc_load(&input.plan_id)?.view)
}

pub fn query(store: &impl ReferenceStore, input: Query) -> Result<Page, EditorError> {
    owner(store)?;
    if !(1..=100).contains(&input.limit) || input.cursor.as_ref().is_some_and(|c| c.len() > 128) {
        return Err(invalid());
    }
    store.gc_query(&input)
}
