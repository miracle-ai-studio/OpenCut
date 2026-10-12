//! Pure selective inversion and causal use cases over complete V4 aggregates.
use super::*;
use crate::history_navigation::{HistoryStep, Navigation};
use opencut_api_contracts::history_navigation::UndoTarget;
use opencut_contracts::project_v4_history::*;
use std::collections::HashMap;

mod merge;
mod navigation;
#[cfg(test)]
mod tests;
pub use navigation::navigate;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HistoryEntryKindV4 {
    Edit,
    Promotion,
    Undo {
        target_transaction_id: String,
    },
    Redo {
        target_transaction_id: String,
        undo_transaction_id: String,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryEntryV4 {
    pub transaction_id: String,
    pub base_revision: String,
    pub revision: String,
    pub label: Option<String>,
    pub kind: HistoryEntryKindV4,
}
pub struct HistorySourceV4 {
    pub source: StoredProjectRevisionV4,
    /// First-parent editor actions, oldest first. Adapter validates causal rows,
    /// revision scope and traversal budgets in the same read snapshot.
    pub entries: Vec<HistoryEntryV4>,
}
pub struct HistoryTransitionV4 {
    pub entry: HistoryEntryV4,
    pub before: StoredProjectRevisionV4,
    pub after: StoredProjectRevisionV4,
}
pub trait ProjectV4HistoryRead: ProjectV4Read {
    fn read_v4_history(
        &self,
        project: &str,
        revision: Option<&str>,
    ) -> Result<HistorySourceV4, EditorError>;
    fn read_v4_history_transition(
        &self,
        project: &str,
        transaction: &str,
    ) -> Result<HistoryTransitionV4, EditorError>;
}

pub struct PlannedHistoryV4 {
    pub plan: HistoryPlanV4,
    /// In-process only; never attach native provenance to the public plan.
    pub proposed_project: Option<ProjectDocumentV4>,
}

fn active_undos(entries: &[HistoryEntryV4]) -> Result<HashMap<String, String>, EditorError> {
    let mut active = HashMap::new();
    let mut originals = std::collections::HashSet::new();
    for entry in entries {
        match &entry.kind {
            HistoryEntryKindV4::Edit => {
                originals.insert(entry.transaction_id.clone());
            }
            HistoryEntryKindV4::Promotion => {}
            HistoryEntryKindV4::Undo {
                target_transaction_id,
            } => {
                if !originals.contains(target_transaction_id)
                    || active.contains_key(target_transaction_id)
                {
                    return Err(EditorError::new(
                        "STORAGE_ERROR",
                        "History undo has no active original edit",
                    ));
                }
                active.insert(target_transaction_id.clone(), entry.transaction_id.clone());
            }
            HistoryEntryKindV4::Redo {
                target_transaction_id,
                undo_transaction_id,
            } => {
                if active.get(target_transaction_id) != Some(undo_transaction_id) {
                    return Err(EditorError::new(
                        "STORAGE_ERROR",
                        "History redo does not match its active undo",
                    ));
                }
                active.remove(target_transaction_id);
            }
        }
    }
    Ok(active)
}

fn read_current(
    store: &impl ProjectV4HistoryRead,
    intent: V4Intent<'_>,
) -> Result<HistorySourceV4, EditorError> {
    store.authorize_v4(intent)?;
    require_idempotency_key(intent.key())?;
    let source = store.read_v4_history(intent.project_id(), None)?;
    let expected = intent
        .expected_revision()
        .ok_or_else(|| invalid("History requires a fixed revision"))?;
    if source.source.revision != expected {
        return Err(revision_conflict(expected, &source.source.revision));
    }
    if source.source.document.project_id() != intent.project_id() {
        return Err(invalid("History project scope differs"));
    }
    if !matches!(source.source.document, StoredProjectDocumentV4::V4(_)) {
        return Err(EditorError::new(
            "UNSUPPORTED_PROJECT_VERSION",
            "Promote the project before using V4 history",
        ));
    }
    Ok(source)
}

fn target<'a>(source: &'a HistorySourceV4, id: &str) -> Result<&'a HistoryEntryV4, EditorError> {
    source
        .entries
        .iter()
        .find(|entry| entry.transaction_id == id)
        .ok_or_else(|| {
            EditorError::new(
                "CAUSAL_CONFLICT",
                "History target is outside the current first-parent ancestry",
            )
        })
}
fn transitioned(
    store: &impl ProjectV4HistoryRead,
    project: &str,
    entry: &HistoryEntryV4,
) -> Result<HistoryTransitionV4, EditorError> {
    let transition = store.read_v4_history_transition(project, &entry.transaction_id)?;
    if transition.entry != *entry
        || transition.before.revision != entry.base_revision
        || transition.after.revision != entry.revision
        || transition.before.document.project_id() != project
        || transition.after.document.project_id() != project
    {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "History transition differs from its scoped journal",
        ));
    }
    Ok(transition)
}
fn plan_on(
    store: &impl ProjectV4HistoryRead,
    intent: V4Intent<'_>,
    source: &HistorySourceV4,
) -> Result<PlannedHistoryV4, EditorError> {
    let active = active_undos(&source.entries)?;
    let (direction, original, undo, transition_entry) = match intent {
        V4Intent::Undo(request) => {
            let entry = target(source, &request.target_transaction_id)?;
            if entry.kind != HistoryEntryKindV4::Edit {
                return Err(invalid(
                    "Promotion and causal actions are not ordinary undo targets",
                ));
            }
            if active.contains_key(&entry.transaction_id) {
                return Err(EditorError::new(
                    "UNDO_UNAVAILABLE",
                    "Target edit is already undone",
                ));
            }
            (
                HistoryDirectionV4::Undo,
                entry.transaction_id.clone(),
                None,
                entry,
            )
        }
        V4Intent::Redo(request) => {
            let entry = target(source, &request.undo_transaction_id)?;
            let HistoryEntryKindV4::Undo {
                target_transaction_id,
            } = &entry.kind
            else {
                return Err(invalid("Redo requires an undo transaction"));
            };
            if active.get(target_transaction_id) != Some(&entry.transaction_id) {
                return Err(EditorError::new(
                    "REDO_UNAVAILABLE",
                    "Undo is not the active causal predecessor",
                ));
            }
            (
                HistoryDirectionV4::Redo,
                target_transaction_id.clone(),
                Some(entry.transaction_id.clone()),
                entry,
            )
        }
        _ => return Err(invalid("Expected a selective history intent")),
    };
    let transition = transitioned(store, intent.project_id(), transition_entry)?;
    let StoredProjectDocumentV4::V4(head) = &source.source.document else {
        return Err(invalid("Expected V4 head"));
    };
    // Only explicitly pinned historical assets or matching known legacy
    // observations can be lifted. The pure converter reports every missing or
    // incompatible binding; it never captures a path or emits partial content.
    let versions = head
        .assets
        .iter()
        .map(|a| (a.id.clone(), a.version.clone()))
        .collect();
    let before = converted(
        &transition.before.document,
        &versions_for(&transition.before.document, &versions),
    )?;
    let after = converted(
        &transition.after.document,
        &versions_for(&transition.after.document, &versions),
    )?;
    // Redo inverts the actual prior selective undo, not a whole-project restore
    // or the old edit's unrelated fields.
    Ok(merge::plan(
        merge::Context {
            direction,
            revision: &source.source.revision,
            original: &original,
            undo: undo.as_deref(),
        },
        &after,
        &before,
        head,
    ))
}
fn versions_for(
    source: &StoredProjectDocumentV4,
    versions: &BTreeMap<String, opencut_contracts::AssetVersion>,
) -> BTreeMap<String, opencut_contracts::AssetVersion> {
    match source {
        StoredProjectDocumentV4::V4(_) => BTreeMap::new(),
        StoredProjectDocumentV4::Legacy(p) => p
            .assets
            .iter()
            .filter(|a| a.version.is_none())
            .filter_map(|a| versions.get(&a.id).map(|v| (a.id.clone(), v.clone())))
            .collect(),
        StoredProjectDocumentV4::V3(p) => p
            .base
            .assets
            .iter()
            .filter(|a| a.version.is_none())
            .filter_map(|a| versions.get(&a.id).map(|v| (a.id.clone(), v.clone())))
            .collect(),
    }
}
pub fn plan_undo(
    store: &impl ProjectV4HistoryRead,
    request: &UndoV4Project,
) -> Result<HistoryPlanV4, EditorError> {
    let intent = V4Intent::Undo(request);
    let source = read_current(store, intent)?;
    Ok(plan_on(store, intent, &source)?.plan)
}
pub fn plan_redo(
    store: &impl ProjectV4HistoryRead,
    request: &RedoV4Project,
) -> Result<HistoryPlanV4, EditorError> {
    let intent = V4Intent::Redo(request);
    let source = read_current(store, intent)?;
    Ok(plan_on(store, intent, &source)?.plan)
}
fn apply_history(
    store: &mut (impl ProjectV4HistoryRead + ProjectV4Store),
    intent: V4Intent<'_>,
) -> Result<HistoryCommitV4, EditorError> {
    store.authorize_v4(intent)?;
    require_idempotency_key(intent.key())?;
    if let Some(transaction) = store.replay_v4_edit(intent)? {
        let (direction, original, undo) = match intent {
            V4Intent::Undo(r) => (
                HistoryDirectionV4::Undo,
                r.target_transaction_id.clone(),
                None,
            ),
            V4Intent::Redo(r) => {
                let transition =
                    store.read_v4_history_transition(&r.project_id, &r.undo_transaction_id)?;
                let HistoryEntryKindV4::Undo {
                    target_transaction_id,
                } = transition.entry.kind
                else {
                    return Err(invalid("Redo receipt has no undo predecessor"));
                };
                (
                    HistoryDirectionV4::Redo,
                    target_transaction_id,
                    Some(r.undo_transaction_id.clone()),
                )
            }
            _ => return Err(invalid("Expected history intent")),
        };
        return Ok(HistoryCommitV4 {
            direction,
            target_transaction_id: original,
            undo_transaction_id: undo,
            transaction,
        });
    }
    let source = read_current(store, intent)?;
    let planned = plan_on(store, intent, &source)?;
    if !planned.plan.conflicts.is_empty() {
        return Err(EditorError {
            code: "SELECTIVE_CONFLICT".into(),
            message: "Target edit overlaps later changes or violates current project constraints"
                .into(),
            details: serde_json::json!({"conflicts":planned.plan.conflicts}),
        });
    }
    let project = planned
        .proposed_project
        .ok_or_else(|| invalid("History plan has no valid project"))?;
    let mut prepared = super::persistence::prepare(&source.source, project, intent)?;
    let journal = match intent {
        V4Intent::Undo(r) => CausalJournalV4::Undo {
            schema_version: 4,
            request: r.clone(),
            changes: planned.plan.changes,
        },
        V4Intent::Redo(r) => CausalJournalV4::Redo {
            schema_version: 4,
            request: r.clone(),
            target_transaction_id: planned.plan.target_transaction_id.clone(),
            changes: planned.plan.changes,
        },
        _ => return Err(invalid("Expected history intent")),
    };
    prepared.journal_json = Some(encode_json(&journal, "V4 causal journal")?);
    let transaction = store.commit_v4_atomic(intent, prepared)?;
    Ok(HistoryCommitV4 {
        direction: planned.plan.direction,
        target_transaction_id: planned.plan.target_transaction_id,
        undo_transaction_id: planned.plan.undo_transaction_id,
        transaction,
    })
}
pub fn undo(
    store: &mut (impl ProjectV4HistoryRead + ProjectV4Store),
    request: &UndoV4Project,
) -> Result<HistoryCommitV4, EditorError> {
    apply_history(store, V4Intent::Undo(request))
}
pub fn redo(
    store: &mut (impl ProjectV4HistoryRead + ProjectV4Store),
    request: &RedoV4Project,
) -> Result<HistoryCommitV4, EditorError> {
    apply_history(store, V4Intent::Redo(request))
}
