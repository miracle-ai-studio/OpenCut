//! Read-only fixed-revision element copy/paste. No browser clipboard, CAS I/O,
//! source-clock edits or independently mutable clipboard state lives here.
use crate::project_v4::{
    ProjectV4Read, V4Intent, public::ProjectV4PublicResources, resources::ProjectV4ResourceRead,
};
use opencut_contracts::{
    EditorError, Time, project_v4::*, project_v4_ops::InspectV4Project, project_v4_public::*,
    timeline_clipboard_v4::*,
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
fn sequence<'a>(project: &'a ProjectDocumentV4, id: &str) -> Result<&'a SequenceV4, EditorError> {
    project
        .sequences
        .iter()
        .find(|s| s.id == id)
        .ok_or_else(|| missing("Clipboard scene is missing"))
}
#[derive(Clone, Serialize)]
struct CopiedItem {
    track_id: String,
    track_kind: TrackKindV4,
    element: TimelineElementV4,
}
fn load_selection(
    snapshot: &ProjectSnapshotV4,
    sequence_id: &str,
    targets: &[ClipboardElementTargetV4],
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
    snapshot: &ProjectSnapshotV4,
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
    store: &(impl ProjectV4Read + ProjectV4ResourceRead),
    input: &CopyTimelineSelectionV4,
) -> Result<TimelineClipboardV4, EditorError> {
    id(&input.project_id)?;
    id(&input.sequence_id)?;
    revision(&input.revision)?;
    store.require_resource_revision_read(&input.project_id, &input.revision)?;
    let snapshot = crate::project_v4::inspect(
        store,
        &InspectV4Project {
            project_id: input.project_id.clone(),
            revision: Some(input.revision.clone()),
        },
    )?;
    let selected = load_selection(&snapshot, &input.sequence_id, &input.targets)?;
    let result = TimelineClipboardV4 {
        schema_version: 1,
        source_project_id: input.project_id.clone(),
        source_revision: input.revision.clone(),
        source_sequence_id: input.sequence_id.clone(),
        items: selected
            .iter()
            .map(|item| TimelineClipboardItemV4 {
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
    store: &(impl ProjectV4PublicResources + ProjectV4ResourceRead),
    input: &PasteTimelineSelectionV4,
) -> Result<PasteTimelinePlanV4, EditorError> {
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
    if input.clipboard.schema_version != 1 {
        return Err(invalid("Unsupported clipboard schema"));
    }
    // A signed exact paste time may be negative: Classic clamps each computed
    // start to zero. Overflow remains an error, never a floating conversion.
    let at = time(&input.at)?;
    let mut transaction = ApplyPublicV4Project {
        project_id: input.project_id.clone(),
        expected_revision: input.expected_revision.clone(),
        idempotency_key: input.idempotency_key.clone(),
        label: input.label.clone(),
        operations: vec![],
    };
    store.authorize_v4(V4Intent::PublicApply(&transaction))?;
    let target = crate::project_v4::inspect(
        store,
        &InspectV4Project {
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
        .map(|item| ClipboardElementTargetV4 {
            track_id: item.source_track_id.clone(),
            element_id: item.source_element_id.clone(),
        })
        .collect();
    let source_copy = CopyTimelineSelectionV4 {
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
    let source = crate::project_v4::inspect(
        store,
        &InspectV4Project {
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
            ClipboardTrackTargetV4::New {
                track_id, name: n, ..
            } => {
                ids.claim(track_id)?;
                name(n)?;
            }
            ClipboardTrackTargetV4::ClassicAboveSource {
                new_track_id,
                name: n,
            } => {
                ids.claim(new_track_id)?;
                name(n)?;
            }
            ClipboardTrackTargetV4::Existing { track_id } => id(track_id)?,
        }
    }
    let mut proposed = target.project.clone();
    let mut decision = PasteTimelineDecisionV4 {
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
    for placement in &input.placements {
        let members: Vec<_> = copied
            .iter()
            .filter(|item| item.track_id == placement.source_track_id)
            .collect();
        let mut elements = Vec::new();
        for (member, requested) in members.iter().zip(&placement.elements) {
            let mut element = member.element.clone();
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
            decision.selection.push(ClipboardElementTargetV4 {
                track_id: target_id.clone(),
                element_id: element.id.clone(),
            });
            decision.actual_starts.push(ClipboardActualStartV4 {
                element_id: element.id.clone(),
                start: element.timeline_range.start.clone(),
            });
        }
    }
    // SequencePut is needed for exact insertion order. Scene-owned data and all
    // existing tracks are cloned unchanged; locks are checked by shared apply.
    transaction
        .operations
        .push(PublicProjectOperationV4::SequencePut {
            sequence: after.clone(),
        });
    *proposed
        .sequences
        .iter_mut()
        .find(|s| s.id == input.sequence_id)
        .ok_or_else(|| missing("Paste destination scene"))? = after;
    opencut_editor_core::project_v4::validate_project_v4(&proposed)?;
    let projected = crate::project_v4::public::plan_public(store, &transaction)?;
    if projected
        != crate::project_v4::public::project(&ProjectSnapshotV4 {
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
    store.authorize_v4(V4Intent::PublicApply(&transaction))?;
    Ok(PasteTimelinePlanV4 {
        decision,
        transaction,
    })
}
