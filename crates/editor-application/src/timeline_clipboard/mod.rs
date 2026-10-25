//! Read-only fixed-revision element copy/paste. No browser clipboard, CAS I/O,
//! source-clock edits or independently mutable clipboard state lives here.
use crate::project_resources::ProjectResourceRead;
use crate::scene::{SceneCommitIntent, SceneRead, public::ProjectPublicResources};
use opencut_contracts::{
    EditorError, Time, project_inspection::InspectProject, project_public::*, scene::*,
    timeline_clipboard::*,
};
use opencut_time::RationalTime as R;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
mod identities;
mod placement;
mod resources;
#[cfg(test)]
mod tests;

const MAX_ITEMS: usize = 60;
fn invalid(message: impl Into<String>) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
fn missing(message: impl Into<String>) -> EditorError {
    EditorError::new("NOT_FOUND", message)
}
fn id(value: &str) -> Result<(), EditorError> {
    if value.trim().is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
        return Err(invalid("Clipboard identity needs 1..128 noncontrol bytes"));
    }
    Ok(())
}
fn revision(value: &str) -> Result<(), EditorError> {
    if value.trim().is_empty() || value.len() > 1024 || value.chars().any(char::is_control) {
        return Err(invalid("Clipboard needs a bounded fixed revision"));
    }
    Ok(())
}
fn name(value: &str) -> Result<(), EditorError> {
    if value.trim().is_empty() || value.len() > 4096 {
        return Err(invalid("New clipboard track needs a 1..4096 byte name"));
    }
    Ok(())
}
fn zero() -> R {
    R::new(0, 1).expect("zero")
}
fn time(value: &Time) -> Result<R, EditorError> {
    R::try_from(value)
}
fn sequence<'a>(project: &'a EditorDocument, id: &str) -> Result<&'a Scene, EditorError> {
    project
        .sequences
        .iter()
        .find(|s| s.id == id)
        .ok_or_else(|| missing("Clipboard scene is missing"))
}
#[derive(Clone, Serialize)]
struct CopiedItem {
    track_id: String,
    track_kind: SceneTrackKind,
    element: TimelineElement,
}
fn load_selection(
    snapshot: &EditorSnapshot,
    sequence_id: &str,
    targets: &[ClipboardElementTarget],
) -> Result<Vec<CopiedItem>, EditorError> {
    if targets.is_empty() || targets.len() > MAX_ITEMS {
        return Err(invalid("Copy needs 1..60 selected elements"));
    }
    let scene = sequence(&snapshot.project, sequence_id)?;
    let mut seen = BTreeSet::new();
    targets
        .iter()
        .map(|target| {
            id(&target.track_id)?;
            id(&target.element_id)?;
            if !seen.insert(&target.element_id) {
                return Err(invalid("Clipboard selection repeats an element"));
            }
            let track = scene
                .tracks
                .iter()
                .find(|t| t.id == target.track_id)
                .ok_or_else(|| missing("Clipboard source track is missing"))?;
            let element = track
                .elements
                .iter()
                .find(|e| e.id == target.element_id)
                .ok_or_else(|| missing("Clipboard source element does not belong to its track"))?;
            // Copy is a Read operation: a locked source is still readable.
            Ok(CopiedItem {
                track_id: track.id.clone(),
                track_kind: track.kind,
                element: element.clone(),
            })
        })
        .collect()
}
fn selection_digest(
    snapshot: &EditorSnapshot,
    scene: &str,
    items: &[CopiedItem],
) -> Result<String, EditorError> {
    let (assets, fonts) = resources::dependencies(&snapshot.project, items)?;
    Ok(crate::identity::request_identity(
        "project_v4_clipboard_selection",
        &snapshot.project.id,
        &(1u32, &snapshot.revision, scene, items, assets, fonts),
    )?
    .current_digest()
    .into())
}

/// References pin the original complete values through an immutable revision.
/// A caller's edited clipboard payload can never replace those stored values.
pub fn copy_selection(
    store: &(impl SceneRead + ProjectResourceRead),
    input: &CopyTimelineSelection,
) -> Result<TimelineClipboard, EditorError> {
    id(&input.project_id)?;
    id(&input.sequence_id)?;
    revision(&input.revision)?;
    store.require_resource_revision_read(&input.project_id, &input.revision)?;
    let snapshot = crate::scene::inspect(
        store,
        &InspectProject {
            project_id: input.project_id.clone(),
            revision: Some(input.revision.clone()),
        },
    )?;
    let selected = load_selection(&snapshot, &input.sequence_id, &input.targets)?;
    let result = TimelineClipboard {
        source_project_id: input.project_id.clone(),
        source_revision: input.revision.clone(),
        source_sequence_id: input.sequence_id.clone(),
        items: selected
            .iter()
            .map(|item| TimelineClipboardItem {
                source_track_id: item.track_id.clone(),
                source_track_kind: item.track_kind,
                source_element_id: item.element.id.clone(),
            })
            .collect(),
        selection_digest: selection_digest(&snapshot, &input.sequence_id, &selected)?,
    };
    store.require_resource_revision_read(&input.project_id, &input.revision)?;
    Ok(result)
}

/// Plans all copied groups atomically. Source and target remain independently
/// fixed; only starts change. Target FPS/canvas and source/local spans do not.
pub fn plan_paste(
    store: &(impl ProjectPublicResources + ProjectResourceRead),
    input: &PasteTimelineSelection,
) -> Result<PasteTimelinePlan, EditorError> {
    plan_selection(store, input, false)
}

/// Classic duplication always creates highest tracks, preserves authored scene
/// times and adds the copy suffix. It is distinct from paste's automatic reuse.
/// The existing paste DTO supplies the fixed references and caller-owned IDs;
/// this new operation leaves the published paste semantics unchanged.
pub fn plan_duplicate(
    store: &(impl ProjectPublicResources + ProjectResourceRead),
    input: &PasteTimelineSelection,
) -> Result<PasteTimelinePlan, EditorError> {
    if input.clipboard.source_project_id != input.project_id
        || input.clipboard.source_sequence_id != input.sequence_id
        || input.clipboard.source_revision != input.expected_revision
    {
        return Err(invalid(
            "Duplicate requires the current destination selection",
        ));
    }
    if input.placements.iter().any(|placement| {
        !matches!(
            placement.target,
            ClipboardTrackTarget::New {
                insertion_anchor: ClipboardTrackInsertion::Highest,
                ..
            }
        )
    }) {
        return Err(invalid(
            "Duplicate requires a new highest track for every source group",
        ));
    }
    plan_selection(store, input, true)
}

fn plan_selection(
    store: &(impl ProjectPublicResources + ProjectResourceRead),
    input: &PasteTimelineSelection,
    duplicate: bool,
) -> Result<PasteTimelinePlan, EditorError> {
    crate::require_idempotency_key(&input.idempotency_key)?;
    id(&input.project_id)?;
    id(&input.sequence_id)?;
    revision(&input.expected_revision)?;
    name(&input.label)?;
    if input.clipboard.source_project_id != input.project_id {
        return Err(EditorError::new(
            "UNSUPPORTED_CLIPBOARD_TRANSFER",
            "Cross-project paste needs an explicit authorized resource-transfer operation",
        ));
    }
    // A signed exact paste time may be negative: Classic clamps each computed
    // start to zero. Overflow remains an error, never a floating conversion.
    let at = time(&input.at)?;
    let mut transaction = ApplyProject {
        branch_name: None,
        project_id: input.project_id.clone(),
        expected_revision: input.expected_revision.clone(),
        idempotency_key: input.idempotency_key.clone(),
        label: input.label.clone(),
        operations: vec![],
    };
    store.authorize_project(SceneCommitIntent::PublicApply(&transaction))?;
    let target = crate::scene::inspect(
        store,
        &InspectProject {
            project_id: input.project_id.clone(),
            revision: None,
        },
    )?;
    if target.revision != input.expected_revision {
        return Err(crate::revision_conflict(
            &input.expected_revision,
            &target.revision,
        ));
    }
    let source_targets: Vec<_> = input
        .clipboard
        .items
        .iter()
        .map(|item| ClipboardElementTarget {
            track_id: item.source_track_id.clone(),
            element_id: item.source_element_id.clone(),
        })
        .collect();
    let source_copy = CopyTimelineSelection {
        project_id: input.clipboard.source_project_id.clone(),
        revision: input.clipboard.source_revision.clone(),
        sequence_id: input.clipboard.source_sequence_id.clone(),
        targets: source_targets,
    };
    let observed = copy_selection(store, &source_copy)?;
    if observed != input.clipboard {
        return Err(EditorError::new(
            "CLIPBOARD_SOURCE_MISMATCH",
            "Clipboard differs from its complete fixed source selection",
        ));
    }
    let source = crate::scene::inspect(
        store,
        &InspectProject {
            project_id: source_copy.project_id.clone(),
            revision: Some(source_copy.revision.clone()),
        },
    )?;
    let copied = load_selection(&source, &source_copy.sequence_id, &source_copy.targets)?;
    let mut group_ids = Vec::new();
    for item in &copied {
        if !group_ids.contains(&item.track_id) {
            group_ids.push(item.track_id.clone());
        }
    }
    if input.placements.len() != group_ids.len() {
        return Err(invalid(
            "Paste must place every copied source-track group exactly once",
        ));
    }
    let min_start = copied
        .iter()
        .map(|item| time(&item.element.timeline_range.start))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .min_by(|a, b| a.cmp_exact(*b))
        .ok_or_else(|| invalid("Empty clipboard"))?;
    if duplicate && at.cmp_exact(min_start).is_ne() {
        return Err(invalid("Duplicate must retain the original scene times"));
    }
    let mut ids = identities::Ids::new(
        &target.project,
        &source.project,
        &input.expected_revision,
        &input.idempotency_key,
    );
    // Claim every explicit ID before generated identities so a late explicit
    // ID cannot collide with an earlier deterministic nested/resource ID.
    for (group_id, placement) in group_ids.iter().zip(&input.placements) {
        if group_id != &placement.source_track_id {
            return Err(invalid("Paste groups must retain clipboard order"));
        }
        let members: Vec<_> = copied
            .iter()
            .filter(|item| item.track_id == *group_id)
            .collect();
        if members.len() != placement.elements.len() {
            return Err(invalid("Paste group must include every copied member"));
        }
        for (member, requested) in members.iter().zip(&placement.elements) {
            if member.element.id != requested.source_element_id {
                return Err(invalid("Paste members must retain clipboard order"));
            }
            ids.claim(&requested.new_element_id)?;
        }
        match &placement.target {
            ClipboardTrackTarget::New {
                track_id, name: n, ..
            } => {
                ids.claim(track_id)?;
                name(n)?;
            }
            ClipboardTrackTarget::ClassicAboveSource {
                new_track_id,
                name: n,
            } => {
                ids.claim(new_track_id)?;
                name(n)?;
            }
            ClipboardTrackTarget::Existing { track_id } => id(track_id)?,
        }
    }
    let mut proposed = target.project.clone();
    let mut decision = PasteTimelineDecision {
        selection: vec![],
        created_ids: vec![],
        new_track_ids: vec![],
        restored_asset_ids: vec![],
        restored_font_ids: vec![],
        actual_starts: vec![],
    };
    let remap = resources::restore(
        &source,
        &copied,
        &mut proposed,
        &mut transaction,
        &mut decision,
        &mut ids,
    )?;
    let mut after = sequence(&proposed, &input.sequence_id)?.clone();
    let mut placements: Vec<_> = input.placements.iter().collect();
    if duplicate {
        // Classic iterates displayed overlays, main, then audio, inserting each
        // duplicate group at Highest. Do not depend on selection/click order.
        let order = placement::display_order(&after);
        placements.sort_by_key(|p| {
            order
                .iter()
                .position(|&i| after.tracks[i].id == p.source_track_id)
        });
    }
    for placement in placements {
        let members: Vec<_> = copied
            .iter()
            .filter(|item| item.track_id == placement.source_track_id)
            .collect();
        let mut elements = Vec::new();
        for (member, requested) in members.iter().zip(&placement.elements) {
            let mut element = member.element.clone();
            if duplicate {
                element.name = format!("{} (copy)", element.name);
            }
            ids.rebind(&mut element, &requested.new_element_id)?;
            remap.apply(&mut element);
            let requested_start =
                at.checked_add(time(&element.timeline_range.start)?.checked_sub(min_start)?)?;
            element.timeline_range.start = if requested_start.cmp_exact(zero()).is_lt() {
                Time::from(zero())
            } else {
                Time::from(requested_start)
            };
            elements.push(element);
        }
        let target_id = placement::apply(
            &mut after,
            &placement.source_track_id,
            members[0].track_kind,
            &placement.target,
            &mut elements,
            &mut decision,
        )?;
        for element in &elements {
            decision.selection.push(ClipboardElementTarget {
                track_id: target_id.clone(),
                element_id: element.id.clone(),
            });
            decision.actual_starts.push(ClipboardActualStart {
                element_id: element.id.clone(),
                start: element.timeline_range.start.clone(),
            });
        }
    }
    // SequencePut is needed for exact insertion order. Scene-owned data and all
    // existing tracks are cloned unchanged; locks are checked by shared apply.
    transaction
        .operations
        .push(ProjectEditOperation::SequencePut {
            sequence: after.clone(),
        });
    *proposed
        .sequences
        .iter_mut()
        .find(|s| s.id == input.sequence_id)
        .ok_or_else(|| missing("Paste destination scene"))? = after;
    opencut_editor_core::scene::validate_project_scene(&proposed)?;
    let projected = crate::scene::public::plan_public(store, &transaction)?;
    if projected
        != crate::scene::public::project(&EditorSnapshot {
            project: proposed.clone(),
            revision: target.revision.clone(),
        })
        .project
    {
        return Err(EditorError::new(
            "INVALID_CLIPBOARD_PLAN",
            "Public paste operations differ from the complete planned aggregate",
        ));
    }
    let old_ids = identities::collect(&target.project);
    decision.created_ids = identities::collect(&proposed)
        .difference(&old_ids)
        .cloned()
        .collect();
    store.require_resource_revision_read(&source_copy.project_id, &source_copy.revision)?;
    store.authorize_project(SceneCommitIntent::PublicApply(&transaction))?;
    Ok(PasteTimelinePlan {
        decision,
        transaction,
    })
}
