//! Pure schema 3 causal merge planning over the shared revision graph.
use opencut_api_contracts::{
    branching::BranchCompareRequest,
    collaboration::{CollaborationConflict, Direction, PropertyChange},
    historical_visual_collaboration::{
        HistoricalVisualBranchComparePlan, HistoricalVisualCollaborationPlan,
    },
};
use opencut_contracts::historical_visual::HistoricalVisualDocument;

fn validate_merge(
    target: &HistoricalVisualDocument,
    proposed: Option<HistoricalVisualDocument>,
    conflicts: &mut Vec<CollaborationConflict>,
) -> Option<HistoricalVisualDocument> {
    proposed.and_then(|project| {
        crate::collaboration::dependency_conflicts(&target.base, &project.base, conflicts);
        if !conflicts.is_empty() {
            return None;
        }
        match opencut_editor_core::historical_visual::validate_project_historical_visual(&project) {
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
    from: &HistoricalVisualDocument,
    to: &HistoricalVisualDocument,
    head: &HistoricalVisualDocument,
) -> HistoricalVisualCollaborationPlan {
    let merged =
        opencut_editor_core::collaboration::selective_merge_historical_visual(from, to, head);
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
    HistoricalVisualCollaborationPlan {
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

pub fn compare_historical_branches(
    request: &BranchCompareRequest,
    base_revision: &str,
    base: &HistoricalVisualDocument,
    source: &HistoricalVisualDocument,
    target: &HistoricalVisualDocument,
) -> HistoricalVisualBranchComparePlan {
    let merged =
        opencut_editor_core::collaboration::selective_merge_historical_visual(base, source, target);
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
    HistoricalVisualBranchComparePlan {
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
