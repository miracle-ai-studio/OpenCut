//! Pure selective inversion and causal use cases over complete V4 aggregates.
use super::*;
use crate::history_navigation::{HistoryStep, Navigation};
use opencut_api_contracts::history_navigation::UndoTarget;
use opencut_contracts::project_history::*;
use std::collections::HashMap;

mod merge;
pub(crate) use merge::merge_documents;
mod navigation;
#[cfg(test)]
mod tests;
pub use navigation::navigate;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HistoryEntryKind {
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
pub struct HistoryEntry {
    pub transaction_id: String,
    pub base_revision: String,
    pub revision: String,
    pub label: Option<String>,
    pub kind: HistoryEntryKind,
}
pub struct HistorySource {
    pub source: StoredProjectRevision,
    /// First-parent editor actions, oldest first. Adapter validates causal rows,
    /// revision scope and traversal budgets in the same read snapshot.
    pub entries: Vec<HistoryEntry>,
}
pub struct HistoryTransition {
    pub entry: HistoryEntry,
    pub before: StoredProjectRevision,
    pub after: StoredProjectRevision,
}
pub trait ProjectHistoryRead: SceneRead {
    fn read_scene_history(
        &self,
        project: &str,
        revision: Option<&str>,
    ) -> Result<HistorySource, EditorError>;
    fn read_scene_history_transition(
        &self,
        project: &str,
        transaction: &str,
    ) -> Result<HistoryTransition, EditorError>;
    /// Optional merged ancestry is resolved on demand,not flattened into UI order.
    fn merged_history_transition(
        &self,
        _project: &str,
        _head: &str,
        _transaction: &str,
    ) -> Result<Option<HistoryTransition>, EditorError> {
        Ok(None)
    }
}

pub struct PlannedHistory {
    pub plan: HistoryPlan,
    /// In-process only; never attach native provenance to the public plan.
    pub proposed_project: Option<EditorDocument>,
}

fn active_undos(
    store: &impl ProjectHistoryRead,
    source: &HistorySource,
) -> Result<HashMap<String, String>, EditorError> {
    let mut active = HashMap::new();
    let mut originals = std::collections::HashSet::new();
    for entry in &source.entries {
        match &entry.kind {
            HistoryEntryKind::Edit => {
                originals.insert(entry.transaction_id.clone());
            }
            HistoryEntryKind::Promotion => {}
            HistoryEntryKind::Undo {
                target_transaction_id,
            } => {
                if !originals.contains(target_transaction_id) {
                    let origin = store.merged_history_transition(
                        source.source.document.project_id(),
                        &entry.base_revision,
                        target_transaction_id,
                    )?;
                    if !origin.is_some_and(|value| {
                        value.entry.kind == HistoryEntryKind::Edit
                            && value.entry.transaction_id == *target_transaction_id
                            && value.before.document.project_id()
                                == source.source.document.project_id()
                            && value.after.document.project_id()
                                == source.source.document.project_id()
                    }) {
                        return Err(EditorError::new(
                            "STORAGE_ERROR",
                            "History undo has no reachable merged original edit",
                        ));
                    }
                    originals.insert(target_transaction_id.clone());
                }
                if active.contains_key(target_transaction_id) {
                    return Err(EditorError::new(
                        "STORAGE_ERROR",
                        "History undo has no active original edit",
                    ));
                }
                active.insert(target_transaction_id.clone(), entry.transaction_id.clone());
            }
            HistoryEntryKind::Redo {
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
    store: &impl ProjectHistoryRead,
    intent: SceneCommitIntent<'_>,
) -> Result<HistorySource, EditorError> {
    store.authorize_project(intent)?;
    require_idempotency_key(intent.key())?;
    let source = store.read_scene_history(intent.project_id(), None)?;
    let expected = intent
        .expected_revision()
        .ok_or_else(|| invalid("History requires a fixed revision"))?;
    if source.source.revision != expected {
        return Err(revision_conflict(expected, &source.source.revision));
    }
    if source.source.document.project_id() != intent.project_id() {
        return Err(invalid("History project scope differs"));
    }
    if !matches!(source.source.document, StoredProjectDocument::Editable(_)) {
        return Err(EditorError::new(
            "UNSUPPORTED_PROJECT_VERSION",
            "Promote the project before using V4 history",
        ));
    }
    Ok(source)
}

fn target(
    source: &HistorySource,
    id: &str,
    merged: Option<&HistoryEntry>,
) -> Result<HistoryEntry, EditorError> {
    source
        .entries
        .iter()
        .chain(merged)
        .find(|entry| entry.transaction_id == id)
        .cloned()
        .ok_or_else(|| {
            EditorError::new(
                "CAUSAL_CONFLICT",
                "History target is outside the published ancestry",
            )
        })
}

fn transitioned(
    store: &impl ProjectHistoryRead,
    project: &str,
    entry: &HistoryEntry,
    prepared: Option<HistoryTransition>,
) -> Result<HistoryTransition, EditorError> {
    let transition = match prepared {
        Some(value) => value,
        None => store.read_scene_history_transition(project, &entry.transaction_id)?,
    };
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
    store: &impl ProjectHistoryRead,
    intent: SceneCommitIntent<'_>,
    source: &HistorySource,
) -> Result<PlannedHistory, EditorError> {
    let active = active_undos(store, source)?;
    let merged = if let SceneCommitIntent::Undo(request) = intent {
        if source
            .entries
            .iter()
            .any(|entry| entry.transaction_id == request.target_transaction_id)
        {
            None
        } else {
            store.merged_history_transition(
                intent.project_id(),
                &source.source.revision,
                &request.target_transaction_id,
            )?
        }
    } else {
        None
    };
    let (direction, original, undo, transition_entry) = match intent {
        SceneCommitIntent::Undo(request) => {
            let entry = target(
                source,
                &request.target_transaction_id,
                merged.as_ref().map(|value| &value.entry),
            )?;
            if entry.kind != HistoryEntryKind::Edit {
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
            (Direction::Undo, entry.transaction_id.clone(), None, entry)
        }
        SceneCommitIntent::Redo(request) => {
            let entry = target(source, &request.undo_transaction_id, None)?;
            let HistoryEntryKind::Undo {
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
                Direction::Redo,
                target_transaction_id.clone(),
                Some(entry.transaction_id.clone()),
                entry,
            )
        }
        _ => return Err(invalid("Expected a selective history intent")),
    };
    let transition = transitioned(store, intent.project_id(), &transition_entry, merged)?;
    let StoredProjectDocument::Editable(head) = &source.source.document else {
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
    source: &StoredProjectDocument,
    versions: &BTreeMap<String, opencut_contracts::AssetVersion>,
) -> BTreeMap<String, opencut_contracts::AssetVersion> {
    match source {
        StoredProjectDocument::Editable(_) => BTreeMap::new(),
        StoredProjectDocument::Legacy(p) => p
            .assets
            .iter()
            .filter(|a| a.version.is_none())
            .filter_map(|a| versions.get(&a.id).map(|v| (a.id.clone(), v.clone())))
            .collect(),
        StoredProjectDocument::HistoricalVisual(p) => p
            .base
            .assets
            .iter()
            .filter(|a| a.version.is_none())
            .filter_map(|a| versions.get(&a.id).map(|v| (a.id.clone(), v.clone())))
            .collect(),
    }
}
pub fn plan_undo(
    store: &impl ProjectHistoryRead,
    request: &UndoRequest,
) -> Result<HistoryPlan, EditorError> {
    let intent = SceneCommitIntent::Undo(request);
    let source = read_current(store, intent)?;
    Ok(plan_on(store, intent, &source)?.plan)
}
pub fn plan_redo(
    store: &impl ProjectHistoryRead,
    request: &RedoRequest,
) -> Result<HistoryPlan, EditorError> {
    let intent = SceneCommitIntent::Redo(request);
    let source = read_current(store, intent)?;
    Ok(plan_on(store, intent, &source)?.plan)
}
fn apply_history(
    store: &mut (impl ProjectHistoryRead + SceneStore),
    intent: SceneCommitIntent<'_>,
) -> Result<HistoryCommit, EditorError> {
    store.authorize_project(intent)?;
    require_idempotency_key(intent.key())?;
    if let Some(transaction) = store.replay_scene_edit(intent)? {
        let (direction, original, undo) = match intent {
            SceneCommitIntent::Undo(r) => (Direction::Undo, r.target_transaction_id.clone(), None),
            SceneCommitIntent::Redo(r) => {
                let transition =
                    store.read_scene_history_transition(&r.project_id, &r.undo_transaction_id)?;
                let HistoryEntryKind::Undo {
                    target_transaction_id,
                } = transition.entry.kind
                else {
                    return Err(invalid("Redo receipt has no undo predecessor"));
                };
                (
                    Direction::Redo,
                    target_transaction_id,
                    Some(r.undo_transaction_id.clone()),
                )
            }
            _ => return Err(invalid("Expected history intent")),
        };
        return Ok(HistoryCommit {
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
        SceneCommitIntent::Undo(r) => CausalJournal::Undo {
            schema_version: 4,
            request: r.clone(),
            changes: planned.plan.changes,
        },
        SceneCommitIntent::Redo(r) => CausalJournal::Redo {
            schema_version: 4,
            request: r.clone(),
            target_transaction_id: planned.plan.target_transaction_id.clone(),
            changes: planned.plan.changes,
        },
        _ => return Err(invalid("Expected history intent")),
    };
    prepared.journal_json = Some(encode_json(&journal, "V4 causal journal")?);
    let transaction = store.commit_scene_atomic(intent, prepared)?;
    Ok(HistoryCommit {
        direction: planned.plan.direction,
        target_transaction_id: planned.plan.target_transaction_id,
        undo_transaction_id: planned.plan.undo_transaction_id,
        transaction,
    })
}
pub fn undo(
    store: &mut (impl ProjectHistoryRead + SceneStore),
    request: &UndoRequest,
) -> Result<HistoryCommit, EditorError> {
    apply_history(store, SceneCommitIntent::Undo(request))
}
pub fn redo(
    store: &mut (impl ProjectHistoryRead + SceneStore),
    request: &RedoRequest,
) -> Result<HistoryCommit, EditorError> {
    apply_history(store, SceneCommitIntent::Redo(request))
}
