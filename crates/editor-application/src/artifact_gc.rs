//! Durable review and exact, fenced execution for one published artifact.
use crate::{access::denied, audit::ExecutionContext, job_views::parse_artifact_id};
pub use opencut_api_contracts::artifact_gc::{
    ExecuteInput, ExecutionState, ExecutionView, Page, Plan, PlanId, PlanInput, Query,
};
use opencut_api_contracts::artifact_retention::ReferenceRoot;
use opencut_contracts::EditorError;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    pub job_id: String,
    pub attempt: u32,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub source: Source,
    pub identity: String,
    pub size_bytes: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub directory_identity: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Removal {
    Removed,
    Absent,
    Changed,
    Busy,
    Retained,
}

pub struct ExecutionSubject {
    pub plan: Plan,
    pub observation: Observation,
    pub receipt: ExecutionView,
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
    fn artifact_execution_subject(&self, plan_id: &str) -> Result<ExecutionSubject, EditorError>;
    fn artifact_execution_mark(
        &mut self,
        input: &ExecuteInput,
        native: &dyn Native,
    ) -> Result<bool, EditorError>;
    fn artifact_execution_finish(
        &mut self,
        input: &ExecuteInput,
        outcome: Removal,
    ) -> Result<ExecutionView, EditorError>;
    fn artifact_execution_inspect(&self, plan_id: &str) -> Result<ExecutionView, EditorError>;
}

pub trait Native {
    fn observe(&self, source: &Source) -> Result<Observation, EditorError>;
    fn matching_roots(
        &self,
        observation: &Observation,
        native_sources: &BTreeMap<String, BTreeSet<ReferenceRoot>>,
    ) -> Result<BTreeSet<ReferenceRoot>, EditorError>;
    fn remove_if(
        &self,
        observation: &Observation,
        still_unreferenced: &mut dyn FnMut() -> Result<bool, EditorError>,
    ) -> Result<Removal, EditorError>;
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

pub fn execute(
    store: &mut impl Store,
    native: &dyn Native,
    input: ExecuteInput,
) -> Result<ExecutionView, EditorError> {
    owner(store)?;
    if input.plan_id.len() > 128
        || !input.plan_id.starts_with("agc_")
        || input.expected_plan_hash.len() != 64
        || !input
            .expected_plan_hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Invalid artifact execution identity",
        ));
    }
    let subject = store.artifact_execution_subject(&input.plan_id)?;
    if subject.plan.plan_hash != input.expected_plan_hash {
        return Err(EditorError::new(
            "PLAN_HASH_MISMATCH",
            "Artifact plan changed or the expected hash is incorrect",
        ));
    }
    if !subject.plan.execution_available || subject.observation.directory_identity.is_none() {
        return Err(EditorError::new(
            "CAPABILITY_UNAVAILABLE",
            "This legacy artifact plan predates safe execution; create a new plan",
        ));
    }
    if subject.plan.default_retained || subject.plan.root_count != "0" {
        return Err(EditorError::new(
            "ARTIFACT_PLAN_PROTECTED",
            "The reviewed plan contains protection roots; release them and create a new plan",
        ));
    }
    if matches!(
        subject.receipt.state,
        ExecutionState::Removed | ExecutionState::Absent | ExecutionState::Changed
    ) {
        return Ok(subject.receipt);
    }
    let outcome = native.remove_if(&subject.observation, &mut || {
        store.artifact_execution_mark(&input, native)
    })?;
    store.artifact_execution_finish(&input, outcome)
}

pub fn execution_inspect(store: &impl Store, input: PlanId) -> Result<ExecutionView, EditorError> {
    owner(store)?;
    if input.plan_id.len() > 128 || !input.plan_id.starts_with("agc_") {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Invalid artifact plan ID",
        ));
    }
    store.artifact_execution_inspect(&input.plan_id)
}
