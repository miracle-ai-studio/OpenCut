//! Pure branch merge planning; native adapters own branch refs and transactions.
use opencut_api_contracts::{
    branching::{BranchComparePlan, BranchCompareRequest},
    collaboration::{CollaborationConflict, PropertyChange},
};
use opencut_contracts::ProjectDocument;

pub fn compare(
    request: &BranchCompareRequest,
    base_revision: &str,
    base: &ProjectDocument,
    source: &ProjectDocument,
    target: &ProjectDocument,
) -> BranchComparePlan {
    let merged = opencut_editor_core::collaboration::selective_merge(base, source, target);
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
    let proposed_project = merged.project.and_then(|project| {
        crate::collaboration::dependency_conflicts(target, &project, &mut conflicts);
        if !conflicts.is_empty() {
            return None;
        }
        match opencut_editor_core::validate_project(&project) {
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
    });
    BranchComparePlan {
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
        proposed_project: if conflicts.is_empty() {
            proposed_project
        } else {
            None
        },
        conflicts,
    }
}
