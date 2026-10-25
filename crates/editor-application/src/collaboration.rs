//! Pure application planning for causal, selective project history edits.
use opencut_api_contracts::collaboration::{
    CollaborationConflict, CollaborationPlan, Direction, PropertyChange,
};
use opencut_contracts::ProjectDocument;
use std::collections::BTreeSet;

pub fn plan(
    direction: Direction,
    expected_revision: &str,
    target_transaction_id: &str,
    undo_transaction_id: Option<&str>,
    from: &ProjectDocument,
    to: &ProjectDocument,
    head: &ProjectDocument,
) -> CollaborationPlan {
    let merged = opencut_editor_core::collaboration::selective_merge(from, to, head);
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
    let proposed_project = merged.project.and_then(|project| {
        dependency_conflicts(head, &project, &mut conflicts);
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
    if merged.changes.is_empty() && conflicts.is_empty() {
        conflicts.push(CollaborationConflict {
            path: "/".into(),
            code: "NO_EFFECT".into(),
            expected: None,
            actual: None,
        });
    }
    let publishable = conflicts.is_empty();
    CollaborationPlan {
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
        proposed_project: if publishable { proposed_project } else { None },
    }
}

pub(crate) fn dependency_conflicts(
    head: &ProjectDocument,
    proposed: &ProjectDocument,
    conflicts: &mut Vec<CollaborationConflict>,
) {
    let kept: BTreeSet<_> = proposed
        .assets
        .iter()
        .map(|asset| asset.id.as_str())
        .collect();
    let removed: BTreeSet<_> = head
        .assets
        .iter()
        .map(|asset| asset.id.as_str())
        .filter(|id| !kept.contains(id))
        .collect();
    if removed.is_empty() {
        return;
    }
    for sequence in &proposed.sequences {
        for track in &sequence.tracks {
            for clip in &track.clips {
                if let Some(asset_id) = clip.asset_id.as_deref()
                    && removed.contains(asset_id)
                {
                    conflicts.push(CollaborationConflict {
                        path: format!(
                            "/sequences/{}/tracks/{}/clips/{}/asset_id",
                            escape(&sequence.id),
                            escape(&track.id),
                            escape(&clip.id)
                        ),
                        code: "ASSET_DEPENDENCY".into(),
                        expected: None,
                        actual: Some(serde_json::Value::String(asset_id.into())),
                    });
                }
            }
        }
        for cue in &sequence.caption_cues {
            if let Some(anchor) = &cue.source_anchor
                && removed.contains(anchor.asset_id.as_str())
            {
                conflicts.push(CollaborationConflict {
                    path: format!(
                        "/sequences/{}/caption_cues/{}/source_anchor/asset_id",
                        escape(&sequence.id),
                        escape(&cue.id)
                    ),
                    code: "ASSET_DEPENDENCY".into(),
                    expected: None,
                    actual: Some(serde_json::Value::String(anchor.asset_id.clone())),
                });
            }
        }
    }
}

fn escape(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn undoing_an_asset_bind_reports_the_later_clip_dependency() {
        let before: ProjectDocument = serde_json::from_value(json!({
            "id":"p","name":"Project","schema_version":2,"default_sequence_id":"main","assets":[],
            "sequences":[{"id":"main","name":"Main","width":640,"height":360,
                "fps":{"numerator":25,"denominator":1},"caption_cues":[],
                "tracks":[{"id":"voice","name":"Voice","kind":"audio","enabled":true,
                    "muted":false,"locked":false,"params":{},"clips":[]}]}
            ]
        }))
        .unwrap();
        let mut after = before.clone();
        after.assets.push(
            serde_json::from_value(json!({
                "id":"speech","name":"Speech","kind":"audio","uri":"opencut-media://speech",
                "duration":{"ticks":"25","timebase":25},"metadata":{}
            }))
            .unwrap(),
        );
        let mut head = after.clone();
        head.sequences[0].tracks[0].clips.push(serde_json::from_value(json!({
            "id":"human-clip","asset_id":"speech",
            "timeline_range":{"start":{"ticks":"0","timebase":25},"duration":{"ticks":"25","timebase":25}},
            "source_range":null,"params":{},"channels":[]
        })).unwrap());
        let result = plan(
            Direction::Undo,
            "rev-head",
            "txn-bind",
            None,
            &after,
            &before,
            &head,
        );
        assert!(result.proposed_project.is_none());
        assert_eq!(result.conflicts[0].code, "ASSET_DEPENDENCY");
        assert_eq!(
            result.conflicts[0].path,
            "/sequences/main/tracks/voice/clips/human-clip/asset_id"
        );
    }
}

/// Original base-document branch comparison,shared by exact archive readers and historical fixture storage.
pub fn compare_legacy_branches(
    request: &opencut_contracts::branching::BranchCompareRequest,
    base_revision: &str,
    base: &ProjectDocument,
    source: &ProjectDocument,
    target: &ProjectDocument,
) -> opencut_api_contracts::branching::HistoricalBranchComparePlan {
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
        dependency_conflicts(target, &project, &mut conflicts);
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
    opencut_api_contracts::branching::HistoricalBranchComparePlan {
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
