//! Durable review plans for one published artifact. This use case does not delete bytes.
use crate::{access::denied, audit::ExecutionContext, job_views::parse_artifact_id};
pub use opencut_api_contracts::artifact_gc::{Page, Plan, PlanId, PlanInput, Query};
use opencut_api_contracts::artifact_retention::ReferenceRoot;
use opencut_contracts::EditorError;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub job_id: String,
    pub attempt: u32,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub source: Source,
    pub identity: String,
    pub size_bytes: String,
}

pub struct Snapshot {
    pub source: Source,
    pub default_retained: bool,
    pub roots: BTreeSet<ReferenceRoot>,
    pub native_sources: BTreeMap<String, BTreeSet<ReferenceRoot>>,
    pub examined_rows: usize,
}

pub trait Store {
    fn artifact_plan_context(&self) -> ExecutionContext;
    fn artifact_plan_replay(&self, input: &PlanInput) -> Result<Option<Plan>, EditorError>;
    fn artifact_plan_snapshot(&self, artifact_id: &str) -> Result<Snapshot, EditorError>;
    fn artifact_plan_create(
        &mut self,
        input: &PlanInput,
        observation: Observation,
        snapshot: Snapshot,
        alias_roots: BTreeSet<ReferenceRoot>,
    ) -> Result<Plan, EditorError>;
    fn artifact_plan_load(&self, plan_id: &str) -> Result<Plan, EditorError>;
    fn artifact_plan_query(&self, input: &Query) -> Result<Page, EditorError>;
}

pub trait Native {
    fn observe(&self, source: &Source) -> Result<Observation, EditorError>;
    fn matching_roots(
        &self,
        observation: &Observation,
        native_sources: &BTreeMap<String, BTreeSet<ReferenceRoot>>,
    ) -> Result<BTreeSet<ReferenceRoot>, EditorError>;
}

fn owner(store: &impl Store) -> Result<(), EditorError> {
    if store.artifact_plan_context().caller().is_owner() {
        Ok(())
    } else {
        Err(denied())
    }
}

pub fn plan(
    store: &mut impl Store,
    native: &impl Native,
    input: PlanInput,
) -> Result<Plan, EditorError> {
    owner(store)?;
    parse_artifact_id(&input.artifact_id)?;
    if input.idempotency_key.trim().is_empty()
        || input.idempotency_key.len() > 1024
        || input.idempotency_key.chars().any(char::is_control)
    {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Invalid artifact plan key",
        ));
    }
    if let Some(previous) = store.artifact_plan_replay(&input)? {
        return Ok(previous);
    }
    let snapshot = store.artifact_plan_snapshot(&input.artifact_id)?;
    let observation = native.observe(&snapshot.source)?;
    let aliases = native.matching_roots(&observation, &snapshot.native_sources)?;
    store.artifact_plan_create(&input, observation, snapshot, aliases)
}

pub fn inspect(store: &impl Store, input: PlanId) -> Result<Plan, EditorError> {
    owner(store)?;
    if input.plan_id.len() > 128 || !input.plan_id.starts_with("agc_") {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Invalid artifact plan ID",
        ));
    }
    store.artifact_plan_load(&input.plan_id)
}

pub fn query(store: &impl Store, input: Query) -> Result<Page, EditorError> {
    owner(store)?;
    if !(1..=100).contains(&input.limit)
        || input
            .cursor
            .as_ref()
            .is_some_and(|cursor| cursor.len() > 132)
    {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Invalid artifact plan page",
        ));
    }
    store.artifact_plan_query(&input)
}
