//! Pure schema 3 causal merge planning over the shared revision graph.
use opencut_api_contracts::{
    branching::BranchCompareRequest,
    collaboration::{CollaborationConflict, Direction, PropertyChange},
    project_v3_collaboration::{V3BranchComparePlan, V3CollaborationPlan},
};
use opencut_contracts::project_v3::ProjectDocumentV3;

fn validate_merge(
    target: &ProjectDocumentV3,
    proposed: Option<ProjectDocumentV3>,
    conflicts: &mut Vec<CollaborationConflict>,
) -> Option<ProjectDocumentV3> {
    proposed.and_then(|project| {
        crate::collaboration::dependency_conflicts(&target.base, &project.base, conflicts);
        if !conflicts.is_empty() {
            return None;
        }
        match opencut_editor_core::project_v3::validate_project_v3(&project) {
            Ok(()) => Some(project),
            Err(error) => {
                conflicts.push(CollaborationConflict {
                    path: "/".into(),
                    code: "PROJECT_CONSTRAINT".into(),
                    expected: None,
                    actual: Some(serde_json::Value::String(error.message)),
                });
                None
            }
        }
    })
}

pub fn plan_history(
    direction: Direction,
    expected_revision: &str,
    target_transaction_id: &str,
    undo_transaction_id: Option<&str>,
    from: &ProjectDocumentV3,
    to: &ProjectDocumentV3,
    head: &ProjectDocumentV3,
) -> V3CollaborationPlan {
    let merged = opencut_editor_core::collaboration::selective_merge_v3(from, to, head);
    let mut conflicts: Vec<_> = merged
        .conflicts
        .into_iter()
        .map(|conflict| CollaborationConflict {
            path: conflict.path,
            code: "DEPENDENT_CHANGE".into(),
            expected: conflict.expected,
            actual: conflict.actual,
        })
        .collect();
    let proposed_project = validate_merge(head, merged.project, &mut conflicts);
    if merged.changes.is_empty() && conflicts.is_empty() {
        conflicts.push(CollaborationConflict {
            path: "/".into(),
            code: "NO_EFFECT".into(),
            expected: None,
            actual: None,
        });
    }
    V3CollaborationPlan {
        direction,
        expected_revision: expected_revision.into(),
        target_transaction_id: target_transaction_id.into(),
        undo_transaction_id: undo_transaction_id.map(str::to_owned),
        changes: merged
            .changes
            .into_iter()
            .map(|change| PropertyChange {
                path: change.path,
                before: change.before,
                after: change.after,
            })
            .collect(),
        conflicts,
        proposed_project,
    }
}

pub fn compare_branches(
    request: &BranchCompareRequest,
    base_revision: &str,
    base: &ProjectDocumentV3,
    source: &ProjectDocumentV3,
    target: &ProjectDocumentV3,
) -> V3BranchComparePlan {
    let merged = opencut_editor_core::collaboration::selective_merge_v3(base, source, target);
    let mut conflicts: Vec<_> = merged
        .conflicts
        .into_iter()
        .map(|conflict| CollaborationConflict {
            path: conflict.path,
            code: "DIVERGENT_CHANGE".into(),
            expected: conflict.expected,
            actual: conflict.actual,
        })
        .collect();
    let proposed_project = validate_merge(target, merged.project, &mut conflicts);
    V3BranchComparePlan {
        project_id: request.project_id.clone(),
        source_branch: request.source_branch.clone(),
        target_branch: request.target_branch.clone(),
        base_revision: base_revision.into(),
        source_revision: request.expected_source_revision.clone(),
        target_revision: request.expected_target_revision.clone(),
        changes: merged
            .changes
            .into_iter()
            .map(|change| PropertyChange {
                path: change.path,
                before: change.before,
                after: change.after,
            })
            .collect(),
        conflicts,
        proposed_project,
    }
}
