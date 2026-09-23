//! Project use cases and transaction-level persistence ports.
use super::{content_id, encode_json, require_idempotency_key, revision_conflict};
use opencut_api_contracts::RestoreRevisionRequest;
use opencut_contracts::{
    Asset, CreateProject, EditPlan, EditorError, Operation, ProjectDocument, ProjectSnapshot,
    TransactionRequest, TransactionResult,
};
use serde::Serialize;
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Clone, Copy)]
pub enum EditIntent<'a> {
    Apply(&'a TransactionRequest),
    Restore(&'a RestoreRevisionRequest),
}
impl<'a> EditIntent<'a> {
    pub fn identity(
        self,
        project_id: &str,
    ) -> Result<crate::identity::RequestIdentity, EditorError> {
        match self {
            Self::Apply(request) => {
                crate::identity::request_identity(self.family(), project_id, request)
            }
            Self::Restore(request) => {
                crate::identity::request_identity(self.family(), project_id, request)
            }
        }
    }
    pub fn family(self) -> &'static str {
        match self {
            Self::Apply(_) => "apply",
            Self::Restore(_) => "restore_revision",
        }
    }
    pub fn key(self) -> &'a str {
        match self {
            Self::Apply(r) => &r.idempotency_key,
            Self::Restore(r) => &r.idempotency_key,
        }
    }
    pub fn expected_revision(self) -> &'a str {
        match self {
            Self::Apply(r) => &r.expected_revision,
            Self::Restore(r) => &r.expected_revision,
        }
    }
}

/// Reads must preserve project scope and immutable historical revisions. Replay
/// compares the original typed request, including the versioned semantic identity;
/// a reused key for a different request returns IDEMPOTENCY_KEY_REUSED.
pub trait ProjectRead {
    /// Check the host-bound caller against current policy before planning or
    /// preparing an edit. Atomic commit must check again under its write lock.
    fn authorize_edit(&self, project_id: &str, intent: EditIntent<'_>) -> Result<(), EditorError>;
    fn head(&self, project_id: &str) -> Result<ProjectSnapshot, EditorError>;
    fn revision(&self, project_id: &str, revision: &str) -> Result<ProjectSnapshot, EditorError>;
    fn replay_create(
        &self,
        input: &CreateProject,
        key: &str,
    ) -> Result<Option<ProjectSnapshot>, EditorError>;
    fn replay_edit(
        &self,
        project_id: &str,
        intent: EditIntent<'_>,
    ) -> Result<Option<TransactionResult>, EditorError>;
}
/// Writes are aggregate transactions, never independent repository saves.
/// Hosts bind a trusted execution context per invocation. Persistence must record
/// committed mutation provenance in the same transaction; a replay retains the
/// original attribution and an audit failure rolls back the business mutation.
pub trait ProjectStore: ProjectRead {
    /// Recheck receipt and project uniqueness, then publish head, initial revision,
    /// receipt and project.created event atomically. Return the original snapshot
    /// on a matching retry, even if the head has subsequently advanced.
    fn create_atomic(
        &mut self,
        input: &CreateProject,
        key: &str,
        snapshot: ProjectSnapshot,
    ) -> Result<ProjectSnapshot, EditorError>;
    /// Under one serialization boundary: recheck authorization, replay intent, compare
    /// expected head, persist revision/head/transaction/receipt/outbox together.
    /// Any failure rolls back all writes. Never re-plan against a newer head.
    fn commit_atomic(
        &mut self,
        intent: EditIntent<'_>,
        prepared: PreparedCommit,
    ) -> Result<TransactionResult, EditorError>;
}
pub trait AssetPinning {
    /// Capture immutable bytes before the commit transaction. Must not mutate
    /// project history or publish a transaction; unused captures may need GC.
    fn pin(&mut self, asset: &Asset) -> Result<Asset, EditorError>;
}
/// Constructed only by application use cases. Adapters can read the immutable
/// payload; there is no public constructor for forging a plan or its base.
pub struct PreparedCommit {
    pub(crate) snapshot: ProjectSnapshot,
    pub(crate) result: TransactionResult,
    pub(crate) request_json: String,
    pub(crate) event_id: String,
}
impl PreparedCommit {
    pub fn snapshot(&self) -> &ProjectSnapshot {
        &self.snapshot
    }
    pub fn result(&self) -> &TransactionResult {
        &self.result
    }
    pub fn request_json(&self) -> &str {
        &self.request_json
    }
    pub fn event_id(&self) -> &str {
        &self.event_id
    }
}

pub fn create(
    store: &mut impl ProjectStore,
    input: CreateProject,
    key: &str,
) -> Result<ProjectSnapshot, EditorError> {
    require_idempotency_key(key)?;
    if let Some(snapshot) = store.replay_create(&input, key)? {
        return Ok(snapshot);
    }
    match store.head(&input.id) {
        Ok(_) => {
            // Another creator may have committed after our first receipt read.
            if let Some(snapshot) = store.replay_create(&input, key)? {
                return Ok(snapshot);
            }
            return Err(EditorError::new(
                "ALREADY_EXISTS",
                format!("project {} already exists", input.id),
            ));
        }
        Err(error) if error.code == "NOT_FOUND" => {}
        Err(error) => return Err(error),
    }
    let project = opencut_editor_core::create_project(&input)?;
    opencut_editor_core::validate_project(&project)?;
    let revision = content_id("rev", encode_json(&project, "project document")?.as_bytes());
    store.create_atomic(&input, key, ProjectSnapshot { project, revision })
}
fn checked_head(
    store: &impl ProjectRead,
    project_id: &str,
    expected: &str,
) -> Result<ProjectSnapshot, EditorError> {
    let snapshot = store.head(project_id)?;
    if snapshot.revision != expected {
        return Err(revision_conflict(expected, &snapshot.revision));
    }
    Ok(snapshot)
}
pub fn plan(
    store: &impl ProjectRead,
    project_id: &str,
    request: &TransactionRequest,
) -> Result<EditPlan, EditorError> {
    store.authorize_edit(project_id, EditIntent::Apply(request))?;
    opencut_editor_core::plan_transaction(
        &checked_head(store, project_id, &request.expected_revision)?,
        request,
    )
}
pub fn apply(
    store: &mut impl ProjectStore,
    assets: &mut impl AssetPinning,
    project_id: &str,
    request: &TransactionRequest,
) -> Result<TransactionResult, EditorError> {
    require_idempotency_key(&request.idempotency_key)?;
    let intent = EditIntent::Apply(request);
    if let Some(result) = store.replay_edit(project_id, intent)? {
        return Ok(result);
    }
    let snapshot = match checked_head(store, project_id, &request.expected_revision) {
        Ok(snapshot) => snapshot,
        Err(error) if error.code == "REVISION_CONFLICT" => {
            if let Some(result) = store.replay_edit(project_id, intent)? {
                return Ok(result);
            }
            return Err(error);
        }
        Err(error) => return Err(error),
    };
    let mut prepared = request.clone();
    for operation in &mut prepared.operations {
        let asset = match operation {
            Operation::BindAsset { asset } | Operation::ReplaceAsset { asset } => asset,
            _ => continue,
        };
        *asset = assets.pin(asset)?;
    }
    let plan = opencut_editor_core::plan_transaction(&snapshot, &prepared)?;
    store.commit_atomic(intent, prepare_commit(&snapshot, plan, &prepared)?)
}
pub fn restore(
    store: &mut impl ProjectStore,
    project_id: &str,
    request: &RestoreRevisionRequest,
) -> Result<TransactionResult, EditorError> {
    require_idempotency_key(&request.idempotency_key)?;
    let intent = EditIntent::Restore(request);
    if let Some(result) = store.replay_edit(project_id, intent)? {
        return Ok(result);
    }
    let snapshot = match checked_head(store, project_id, &request.expected_revision) {
        Ok(snapshot) => snapshot,
        Err(error) if error.code == "REVISION_CONFLICT" => {
            if let Some(result) = store.replay_edit(project_id, intent)? {
                return Ok(result);
            }
            return Err(error);
        }
        Err(error) => return Err(error),
    };
    let project = store
        .revision(project_id, &request.target_revision)?
        .project;
    opencut_editor_core::validate_project(&project)?;
    let before = project_entities(&snapshot.project);
    let after = project_entities(&project);
    let plan = EditPlan {
        base_revision: snapshot.revision.clone(),
        created_ids: after
            .keys()
            .filter(|id| !before.contains_key(*id))
            .cloned()
            .collect(),
        deleted_ids: before
            .keys()
            .filter(|id| !after.contains_key(*id))
            .cloned()
            .collect(),
        changed_ids: after
            .iter()
            .filter(|(id, value)| before.get(*id).is_some_and(|old| old != *value))
            .map(|(id, _)| id.clone())
            .collect(),
        unresolved_asset_ids: Vec::new(),
        proposed_project: project,
    };
    store.commit_atomic(intent, prepare_commit(&snapshot, plan, request)?)
}
fn prepare_commit(
    snapshot: &ProjectSnapshot,
    plan: EditPlan,
    request: &impl Serialize,
) -> Result<PreparedCommit, EditorError> {
    opencut_editor_core::validate_project(&plan.proposed_project)?;
    let transaction_id = format!("txn_{}", Uuid::new_v4().simple());
    let material = encode_json(
        &json!({"parent":snapshot.revision, "transaction_id":transaction_id, "document":plan.proposed_project}),
        "revision material",
    )?;
    let revision = content_id("rev", material.as_bytes());
    Ok(PreparedCommit {
        snapshot: ProjectSnapshot {
            project: plan.proposed_project,
            revision: revision.clone(),
        },
        result: TransactionResult {
            transaction_id,
            base_revision: snapshot.revision.clone(),
            revision,
            created_ids: plan.created_ids,
            changed_ids: plan.changed_ids,
            deleted_ids: plan.deleted_ids,
        },
        request_json: encode_json(request, "transaction request")?,
        event_id: format!("evt_{}", Uuid::new_v4().simple()),
    })
}
fn project_entities(project: &ProjectDocument) -> std::collections::BTreeMap<String, Value> {
    let mut objects = std::collections::BTreeMap::new();
    objects.insert(project.id.clone(), json!(project));
    for asset in &project.assets {
        objects.insert(asset.id.clone(), json!(asset));
    }
    for sequence in &project.sequences {
        objects.insert(sequence.id.clone(), json!(sequence));
        for cue in &sequence.caption_cues {
            objects.insert(cue.id.clone(), json!(cue));
        }
        for track in &sequence.tracks {
            objects.insert(track.id.clone(), json!(track));
            for clip in &track.clips {
                objects.insert(clip.id.clone(), json!(clip));
            }
        }
    }
    objects
}
