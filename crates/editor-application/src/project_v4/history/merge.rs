use super::*;
use sha2::{Digest, Sha256};

fn digest(value: Option<&serde_json::Value>) -> Option<String> {
    value.map(|v| {
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(v).expect("typed history value serializes"))
        )
    })
}
fn locks(current: &ProjectDocumentV4, proposed: &ProjectDocumentV4) -> Result<(), EditorError> {
    // Use the same lock semantics as ordinary granular editing. Lock-only
    // inversion is allowed; content edits cannot silently unlock a locked track.
    for asset in &current.assets {
        match proposed.assets.iter().find(|a| a.id == asset.id) {
            Some(value) if value != asset => super::super::locks::check(
                current,
                &ProjectOperationV4::AssetPut {
                    asset: value.clone(),
                },
            )?,
            None => super::super::locks::check(
                current,
                &ProjectOperationV4::AssetDelete {
                    asset_id: asset.id.clone(),
                },
            )?,
            _ => {}
        }
    }
    for font in &current.fonts {
        match proposed.fonts.iter().find(|f| f.id == font.id) {
            Some(value) if value != font => super::super::locks::check(
                current,
                &ProjectOperationV4::FontPut {
                    font: value.clone(),
                },
            )?,
            None => super::super::locks::check(
                current,
                &ProjectOperationV4::FontDelete {
                    font_id: font.id.clone(),
                },
            )?,
            _ => {}
        }
    }
    for sequence in &current.sequences {
        match proposed.sequences.iter().find(|s| s.id == sequence.id) {
            None => super::super::locks::check(
                current,
                &ProjectOperationV4::SequenceDelete {
                    sequence_id: sequence.id.clone(),
                },
            )?,
            Some(value) => {
                let mut value = value.clone();
                for track in &mut value.tracks {
                    if let Some(old) = sequence.tracks.iter().find(|t| t.id == track.id) {
                        track.locked = old.locked;
                    }
                }
                super::super::locks::check(
                    current,
                    &ProjectOperationV4::SequencePut { sequence: value },
                )?;
            }
        }
    }
    Ok(())
}
pub(super) struct Context<'a> {
    pub direction: HistoryDirectionV4,
    pub revision: &'a str,
    pub original: &'a str,
    pub undo: Option<&'a str>,
}
pub(super) fn plan(
    context: Context<'_>,
    from: &ProjectDocumentV4,
    to: &ProjectDocumentV4,
    head: &ProjectDocumentV4,
) -> PlannedHistoryV4 {
    let merged = opencut_editor_core::collaboration::selective_merge_v4(from, to, head);
    let mut conflicts: Vec<_> = merged
        .conflicts
        .iter()
        .map(|c| HistoryConflictV4 {
            path: c.path.clone(),
            reason: HistoryConflictReasonV4::DependentChange,
            expected_sha256: digest(c.expected.as_ref()),
            actual_sha256: digest(c.actual.as_ref()),
        })
        .collect();
    let changes: Vec<_> = merged
        .changes
        .iter()
        .map(|c| HistoryChangeV4 {
            path: c.path.clone(),
            before_sha256: digest(c.before.as_ref()),
            after_sha256: digest(c.after.as_ref()),
        })
        .collect();
    let mut project = merged.project;
    if let Some(proposed) = &project {
        let reason = if validate_project_v4(proposed).is_err() {
            Some(HistoryConflictReasonV4::ProjectConstraint)
        } else if locks(head, proposed).is_err() {
            Some(HistoryConflictReasonV4::LockedTrack)
        } else {
            None
        };
        if let Some(reason) = reason {
            conflicts.push(HistoryConflictV4 {
                path: "/".into(),
                reason,
                expected_sha256: None,
                actual_sha256: None,
            });
        }
    }
    if changes.is_empty() && conflicts.is_empty() {
        conflicts.push(HistoryConflictV4 {
            path: "/".into(),
            reason: HistoryConflictReasonV4::NoEffect,
            expected_sha256: None,
            actual_sha256: None,
        });
    }
    if !conflicts.is_empty() {
        project = None;
    }
    PlannedHistoryV4 {
        plan: HistoryPlanV4 {
            direction: context.direction,
            expected_revision: context.revision.into(),
            target_transaction_id: context.original.into(),
            undo_transaction_id: context.undo.map(str::to_owned),
            changes,
            conflicts,
        },
        proposed_project: project,
    }
}
