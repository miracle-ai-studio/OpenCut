//! One branch workflow over the ordinary complete editor transaction kernel.
use crate::scene::{
    self, SceneCommitIntent, SceneRead, SceneStore, StoredProjectDocument, StoredProjectRevision,
};
use crate::{require_idempotency_key, revision_conflict};
use opencut_contracts::{
    EditorError,
    branching::*,
    scene::{EditorDocument, EditorSnapshot},
};

pub struct ComparisonSource {
    pub base: StoredProjectRevision,
    pub source: StoredProjectRevision,
    pub target: StoredProjectRevision,
}
pub trait BranchStore: SceneRead {
    /// One current-authority read of metadata;no old document projection.
    fn read_branch_refs(&self, project: &str) -> Result<Vec<BranchView>, EditorError>;
    fn create_branch_ref(&mut self, input: &BranchCreateRequest)
    -> Result<BranchView, EditorError>;
    /// Both expected refs,common ancestor and immutable documents in one read.
    fn comparison_source(
        &self,
        input: &BranchCompareRequest,
    ) -> Result<ComparisonSource, EditorError>;
    fn replay_branch_merge(
        &self,
        input: &BranchMergeRequest,
    ) -> Result<Option<BranchMergeCommit>, EditorError>;
}
pub fn validate_name(value: &str, main_allowed: bool) -> Result<(), EditorError> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        || (!main_allowed && value == "main")
    {
        return Err(EditorError::new("INVALID_ARGUMENT", "Invalid branch name"));
    }
    Ok(())
}
pub fn validate_compare(input: &BranchCompareRequest) -> Result<(), EditorError> {
    validate_name(&input.source_branch, true)?;
    validate_name(&input.target_branch, true)?;
    if input.source_branch == input.target_branch
        || [
            &input.project_id,
            &input.expected_source_revision,
            &input.expected_target_revision,
        ]
        .iter()
        .any(|v| v.trim().is_empty())
    {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Compare two distinct branches at nonempty fixed revisions",
        ));
    }
    Ok(())
}
pub fn comparison(input: &BranchMergeRequest) -> BranchCompareRequest {
    BranchCompareRequest {
        base_asset_versions: input.base_asset_versions.clone(),
        project_id: input.project_id.clone(),
        source_branch: input.source_branch.clone(),
        target_branch: input.target_branch.clone(),
        expected_source_revision: input.expected_source_revision.clone(),
        expected_target_revision: input.expected_target_revision.clone(),
    }
}
pub fn list(
    store: &impl BranchStore,
    input: &BranchListRequest,
) -> Result<Vec<BranchView>, EditorError> {
    store.read_branch_refs(&input.project_id)
}
pub fn create(
    store: &mut impl BranchStore,
    input: &BranchCreateRequest,
) -> Result<BranchView, EditorError> {
    validate_name(&input.branch_name, false)?;
    require_idempotency_key(&input.idempotency_key)?;
    if [
        &input.project_id,
        &input.expected_revision,
        &input.from_revision,
    ]
    .iter()
    .any(|v| v.trim().is_empty())
    {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Branch creation requires a project and fixed revisions",
        ));
    }
    store.create_branch_ref(input)
}
fn editor<'a>(
    source: &'a StoredProjectRevision,
    project: &str,
) -> Result<&'a EditorDocument, EditorError> {
    if source.document.project_id() != project {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Branch document differs from its project scope",
        ));
    }
    let StoredProjectDocument::Editable(document) = &source.document else {
        return Err(EditorError::new(
            "PROJECT_READ_ONLY",
            "Historical branch content must be explicitly promoted before merge",
        ));
    };
    Ok(document)
}
struct ProposedMerge {
    plan: BranchComparePlan,
    target: StoredProjectRevision,
    document: Option<EditorDocument>,
}
fn proposal(
    store: &impl BranchStore,
    input: &BranchCompareRequest,
) -> Result<ProposedMerge, EditorError> {
    validate_compare(input)?;
    let sources = store.comparison_source(input)?;
    if sources.source.revision != input.expected_source_revision {
        return Err(revision_conflict(
            &input.expected_source_revision,
            &sources.source.revision,
        ));
    }
    if sources.target.revision != input.expected_target_revision {
        return Err(revision_conflict(
            &input.expected_target_revision,
            &sources.target.revision,
        ));
    }
    let base = scene::converted(&sources.base.document, &input.base_asset_versions)?;
    let source = editor(&sources.source, &input.project_id)?;
    let target = editor(&sources.target, &input.project_id)?;
    let merged = scene::history::merge_documents(&base, source, target);
    let public = merged.project.as_ref().map(|document| {
        scene::public::project(&EditorSnapshot {
            project: document.clone(),
            revision: input.expected_target_revision.clone(),
        })
        .project
    });
    Ok(ProposedMerge {
        plan: BranchComparePlan {
            project_id: input.project_id.clone(),
            source_branch: input.source_branch.clone(),
            target_branch: input.target_branch.clone(),
            base_revision: sources.base.revision,
            source_revision: sources.source.revision,
            target_revision: sources.target.revision.clone(),
            changes: merged.changes,
            conflicts: merged.conflicts,
            proposed_project: public,
        },
        target: sources.target,
        document: merged.project,
    })
}
pub fn compare(
    store: &impl BranchStore,
    input: &BranchCompareRequest,
) -> Result<BranchComparePlan, EditorError> {
    Ok(proposal(store, input)?.plan)
}
pub fn merge(
    store: &mut (impl BranchStore + SceneStore),
    input: &BranchMergeRequest,
) -> Result<BranchMergeCommit, EditorError> {
    require_idempotency_key(&input.idempotency_key)?;
    validate_compare(&comparison(input))?;
    let intent = SceneCommitIntent::BranchMerge(input);
    store.authorize_project(intent)?;
    if let Some(receipt) = store.replay_branch_merge(input)? {
        return Ok(receipt);
    }
    let proposed = proposal(store, &comparison(input))?;
    let document = proposed.document.ok_or_else(|| {
        EditorError::new("CAUSAL_CONFLICT", "Branch merge has conflicts or no effect")
            .with_details(serde_json::json!({"plan":&proposed.plan}))
    })?;
    let prepared = scene::prepare_branch_merge(
        &proposed.target,
        document,
        input,
        &proposed.plan.base_revision,
        &proposed.plan.changes,
    )?;
    let transaction = store.commit_scene_atomic(intent, prepared)?;
    Ok(BranchMergeCommit {
        source_branch: input.source_branch.clone(),
        target_branch: input.target_branch.clone(),
        base_revision: proposed.plan.base_revision,
        source_revision: input.expected_source_revision.clone(),
        transaction,
    })
}

mod graph;
pub use graph::common_ancestor;
