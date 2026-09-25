//! Pure schema 3 aggregate edits. SQLite owns CAS, identity, audit and publication.
use crate::access::SourceAccess;
use opencut_contracts::{
    CreateProject, EditorError, ProjectDocument, ProjectSnapshot, TransactionRequest,
    project_v3::{
        EffectNode, MaskNode, ProjectDocumentV3, SequenceVisualV3, ShapeClip, TransitionNode,
    },
    project_v3_ops::{ApplyV3Project, VisualOperation, VisualPlan},
};
use std::collections::BTreeSet;

/// A fixed v3 revision paired with source-path authority derived from host identity.
#[derive(Clone, Debug)]
pub struct AuthorizedSnapshotV3 {
    pub snapshot: opencut_contracts::project_v3::ProjectSnapshotV3,
    pub sources: SourceAccess,
}

fn invalid(message: impl Into<String>) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}

pub fn create(input: &CreateProject) -> Result<ProjectDocumentV3, EditorError> {
    let base = opencut_editor_core::create_project(input)?;
    promote(&base)
}

pub fn promote(base: &ProjectDocument) -> Result<ProjectDocumentV3, EditorError> {
    if base.schema_version != 2 {
        return Err(invalid("Schema 3 promotion requires a schema 2 project"));
    }
    let project = ProjectDocumentV3 {
        schema_version: 3,
        base: base.clone(),
        visual: Vec::new(),
    };
    opencut_editor_core::project_v3::validate_project_v3(&project)?;
    Ok(project)
}

trait Entity {
    fn id(&self) -> &str;
}
impl Entity for ShapeClip {
    fn id(&self) -> &str {
        &self.id
    }
}
impl Entity for EffectNode {
    fn id(&self) -> &str {
        &self.id
    }
}
impl Entity for MaskNode {
    fn id(&self) -> &str {
        &self.id
    }
}
impl Entity for TransitionNode {
    fn id(&self) -> &str {
        &self.id
    }
}

fn put<T: Entity + PartialEq>(
    items: &mut Vec<T>,
    value: T,
    created: &mut Vec<String>,
    changed: &mut Vec<String>,
) {
    if let Some(index) = items.iter().position(|item| item.id() == value.id()) {
        if items[index] != value {
            changed.push(value.id().into());
            items[index] = value;
        }
    } else {
        created.push(value.id().into());
        items.push(value);
    }
}
fn delete<T: Entity>(
    items: &mut Vec<T>,
    id: &str,
    deleted: &mut Vec<String>,
) -> Result<(), EditorError> {
    let index = items
        .iter()
        .position(|item| item.id() == id)
        .ok_or_else(|| EditorError::new("NOT_FOUND", format!("Visual node {id} does not exist")))?;
    items.remove(index);
    deleted.push(id.into());
    Ok(())
}
fn sequence<'a>(
    project: &'a mut ProjectDocumentV3,
    id: &str,
) -> Result<&'a mut SequenceVisualV3, EditorError> {
    if !project.base.sequences.iter().any(|s| s.id == id) {
        return Err(EditorError::new("NOT_FOUND", "Sequence does not exist"));
    }
    if let Some(index) = project.visual.iter().position(|s| s.sequence_id == id) {
        return Ok(&mut project.visual[index]);
    }
    project.visual.push(SequenceVisualV3 {
        sequence_id: id.into(),
        shapes: Vec::new(),
        effects: Vec::new(),
        masks: Vec::new(),
        transitions: Vec::new(),
    });
    Ok(project.visual.last_mut().expect("inserted visual sequence"))
}

fn unlocked_track(
    project: &ProjectDocumentV3,
    sequence_id: &str,
    track_id: &str,
) -> Result<(), EditorError> {
    let track = project
        .base
        .sequences
        .iter()
        .find(|s| s.id == sequence_id)
        .and_then(|s| s.tracks.iter().find(|t| t.id == track_id))
        .ok_or_else(|| EditorError::new("NOT_FOUND", format!("Track {track_id} does not exist")))?;
    if track.locked {
        return Err(EditorError::new(
            "CONSTRAINT_VIOLATION",
            format!("track {track_id} is locked"),
        ));
    }
    Ok(())
}

fn target_track<'a>(
    project: &'a ProjectDocumentV3,
    sequence_id: &str,
    target_id: &str,
) -> Result<&'a str, EditorError> {
    let base = project
        .base
        .sequences
        .iter()
        .find(|s| s.id == sequence_id)
        .ok_or_else(|| EditorError::new("NOT_FOUND", "Sequence does not exist"))?;
    if let Some(track) = base
        .tracks
        .iter()
        .find(|t| t.clips.iter().any(|c| c.id == target_id))
    {
        return Ok(&track.id);
    }
    project
        .visual
        .iter()
        .find(|s| s.sequence_id == sequence_id)
        .and_then(|s| s.shapes.iter().find(|shape| shape.id == target_id))
        .map(|shape| shape.track_id.as_str())
        .ok_or_else(|| EditorError::new("NOT_FOUND", format!("Target {target_id} does not exist")))
}

fn unlocked_target(
    project: &ProjectDocumentV3,
    sequence_id: &str,
    target_id: &str,
) -> Result<(), EditorError> {
    unlocked_track(
        project,
        sequence_id,
        target_track(project, sequence_id, target_id)?,
    )
}

fn check_visual_lock(
    project: &ProjectDocumentV3,
    operation: &VisualOperation,
) -> Result<(), EditorError> {
    let Some(sequence_id) = operation.sequence_id() else {
        return Ok(());
    };
    let visual = project.visual.iter().find(|s| s.sequence_id == sequence_id);
    match operation {
        VisualOperation::ShapePut { shape, .. } => {
            if let Some(old) = visual.and_then(|s| s.shapes.iter().find(|v| v.id == shape.id)) {
                unlocked_track(project, sequence_id, &old.track_id)?;
            }
            unlocked_track(project, sequence_id, &shape.track_id)
        }
        VisualOperation::ShapeDelete { id, .. } => {
            let old = visual
                .and_then(|s| s.shapes.iter().find(|v| v.id == *id))
                .ok_or_else(|| {
                    EditorError::new("NOT_FOUND", format!("Visual node {id} does not exist"))
                })?;
            unlocked_track(project, sequence_id, &old.track_id)
        }
        VisualOperation::EffectPut { effect, .. } => {
            if let Some(old) = visual.and_then(|s| s.effects.iter().find(|v| v.id == effect.id)) {
                unlocked_target(project, sequence_id, &old.target_id)?;
            }
            unlocked_target(project, sequence_id, &effect.target_id)
        }
        VisualOperation::EffectDelete { id, .. } => {
            let old = visual
                .and_then(|s| s.effects.iter().find(|v| v.id == *id))
                .ok_or_else(|| {
                    EditorError::new("NOT_FOUND", format!("Visual node {id} does not exist"))
                })?;
            unlocked_target(project, sequence_id, &old.target_id)
        }
        VisualOperation::MaskPut { mask, .. } => {
            if let Some(old) = visual.and_then(|s| s.masks.iter().find(|v| v.id == mask.id)) {
                unlocked_target(project, sequence_id, &old.target_id)?;
            }
            unlocked_target(project, sequence_id, &mask.target_id)
        }
        VisualOperation::MaskDelete { id, .. } => {
            let old = visual
                .and_then(|s| s.masks.iter().find(|v| v.id == *id))
                .ok_or_else(|| {
                    EditorError::new("NOT_FOUND", format!("Visual node {id} does not exist"))
                })?;
            unlocked_target(project, sequence_id, &old.target_id)
        }
        VisualOperation::TransitionPut { transition, .. } => {
            if let Some(old) =
                visual.and_then(|s| s.transitions.iter().find(|v| v.id == transition.id))
            {
                unlocked_target(project, sequence_id, &old.outgoing_clip_id)?;
                unlocked_target(project, sequence_id, &old.incoming_clip_id)?;
            }
            unlocked_target(project, sequence_id, &transition.outgoing_clip_id)?;
            unlocked_target(project, sequence_id, &transition.incoming_clip_id)
        }
        VisualOperation::TransitionDelete { id, .. } => {
            let old = visual
                .and_then(|s| s.transitions.iter().find(|v| v.id == *id))
                .ok_or_else(|| {
                    EditorError::new("NOT_FOUND", format!("Visual node {id} does not exist"))
                })?;
            unlocked_target(project, sequence_id, &old.outgoing_clip_id)?;
            unlocked_target(project, sequence_id, &old.incoming_clip_id)
        }
        VisualOperation::Base { .. } => Ok(()),
    }
}

fn entity_value(project: &ProjectDocumentV3, id: &str) -> Option<serde_json::Value> {
    if project.base.id == id {
        return serde_json::to_value(&project.base).ok();
    }
    for asset in &project.base.assets {
        if asset.id == id {
            return serde_json::to_value(asset).ok();
        }
    }
    for sequence in &project.base.sequences {
        if sequence.id == id {
            return serde_json::to_value(sequence).ok();
        }
        for cue in &sequence.caption_cues {
            if cue.id == id {
                return serde_json::to_value(cue).ok();
            }
        }
        for track in &sequence.tracks {
            if track.id == id {
                return serde_json::to_value(track).ok();
            }
            for clip in &track.clips {
                if clip.id == id {
                    return serde_json::to_value(clip).ok();
                }
            }
        }
    }
    for visual in &project.visual {
        for shape in &visual.shapes {
            if shape.id == id {
                return serde_json::to_value(shape).ok();
            }
        }
        for effect in &visual.effects {
            if effect.id == id {
                return serde_json::to_value(effect).ok();
            }
        }
        for mask in &visual.masks {
            if mask.id == id {
                return serde_json::to_value(mask).ok();
            }
        }
        for transition in &visual.transitions {
            if transition.id == id {
                return serde_json::to_value(transition).ok();
            }
        }
    }
    None
}

pub fn plan_apply(
    current: &ProjectDocumentV3,
    request: &ApplyV3Project,
) -> Result<VisualPlan, EditorError> {
    if current.base.id != request.project_id
        || request.operations.is_empty()
        || request.operations.len() > 128
        || request.label.trim().is_empty()
        || request.label.len() > 256
    {
        return Err(invalid("Invalid schema 3 edit request"));
    }
    opencut_editor_core::project_v3::validate_project_v3(current)?;
    let mut proposed = current.clone();
    let (mut created, mut changed, mut deleted) = (Vec::new(), Vec::new(), Vec::new());
    for operation in &request.operations {
        if let VisualOperation::Base {
            operation: base_operation,
        } = operation
        {
            let base_request = TransactionRequest {
                expected_revision: request.expected_revision.clone(),
                idempotency_key: request.idempotency_key.clone(),
                label: request.label.clone(),
                operations: vec![base_operation.clone()],
            };
            let planned = opencut_editor_core::plan_transaction(
                &ProjectSnapshot {
                    project: proposed.base.clone(),
                    revision: request.expected_revision.clone(),
                },
                &base_request,
            )?;
            proposed.base = planned.proposed_project;
            created.extend(planned.created_ids);
            changed.extend(planned.changed_ids);
            deleted.extend(planned.deleted_ids);
            continue;
        }
        check_visual_lock(&proposed, operation)?;
        let visual = sequence(
            &mut proposed,
            operation.sequence_id().expect("non-base visual operation"),
        )?;
        match operation {
            VisualOperation::Base { .. } => unreachable!(),
            VisualOperation::ShapePut { shape, .. } => put(
                &mut visual.shapes,
                shape.clone(),
                &mut created,
                &mut changed,
            ),
            VisualOperation::ShapeDelete { id, .. } => {
                delete(&mut visual.shapes, id, &mut deleted)?
            }
            VisualOperation::EffectPut { effect, .. } => put(
                &mut visual.effects,
                effect.clone(),
                &mut created,
                &mut changed,
            ),
            VisualOperation::EffectDelete { id, .. } => {
                delete(&mut visual.effects, id, &mut deleted)?
            }
            VisualOperation::MaskPut { mask, .. } => {
                put(&mut visual.masks, mask.clone(), &mut created, &mut changed)
            }
            VisualOperation::MaskDelete { id, .. } => delete(&mut visual.masks, id, &mut deleted)?,
            VisualOperation::TransitionPut { transition, .. } => put(
                &mut visual.transitions,
                transition.clone(),
                &mut created,
                &mut changed,
            ),
            VisualOperation::TransitionDelete { id, .. } => {
                delete(&mut visual.transitions, id, &mut deleted)?
            }
        }
    }
    if proposed == *current || (created.is_empty() && changed.is_empty() && deleted.is_empty()) {
        return Err(EditorError::new("NO_EFFECT", "Visual edit has no effect"));
    }
    opencut_editor_core::project_v3::validate_project_v3(&proposed)?;
    let ids: BTreeSet<_> = created.into_iter().chain(changed).chain(deleted).collect();
    let (mut created, mut changed, mut deleted) = (Vec::new(), Vec::new(), Vec::new());
    for id in ids {
        match (entity_value(current, &id), entity_value(&proposed, &id)) {
            (None, Some(_)) => created.push(id),
            (Some(_), None) => deleted.push(id),
            (Some(before), Some(after)) if before != after => changed.push(id),
            _ => {}
        }
    }
    if created.is_empty() && changed.is_empty() && deleted.is_empty() {
        changed.push(current.base.id.clone());
    }
    Ok(VisualPlan {
        proposed_project: proposed,
        created_ids: created,
        changed_ids: changed,
        deleted_ids: deleted,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencut_contracts::FrameRate;
    use serde_json::json;

    fn project(locked: bool) -> ProjectDocumentV3 {
        let mut project = create(&CreateProject {
            id: "p".into(),
            name: "P".into(),
            sequence_id: "s".into(),
            width: 640,
            height: 360,
            fps: FrameRate {
                numerator: 25,
                denominator: 1,
            },
        })
        .unwrap();
        project.base.sequences[0].tracks.push(
            serde_json::from_value(json!({
                "id":"graphics","name":"Graphics","kind":"graphic","enabled":true,
                "muted":false,"locked":locked,"clips":[],"params":{}
            }))
            .unwrap(),
        );
        project
    }
    fn shape() -> ShapeClip {
        serde_json::from_value(json!({"id":"card","track_id":"graphics",
            "timeline_range":{"start":{"ticks":"0","timebase":1000},
            "duration":{"ticks":"1000","timebase":1000}},
            "primitive":{"kind":"rectangle","width_px":200.0,"height_px":100.0},
            "fill":{"red":255,"green":0,"blue":0,"alpha":255},
            "position_x":0.0,"position_y":0.0,"rotation_degrees":0.0,"opacity":1.0}))
        .unwrap()
    }
    fn request(operations: Vec<VisualOperation>) -> ApplyV3Project {
        ApplyV3Project {
            project_id: "p".into(),
            expected_revision: "r".into(),
            idempotency_key: "k".into(),
            label: "Edit".into(),
            operations,
        }
    }
    #[test]
    fn locked_track_blocks_visual_create_and_existing_target_edits() {
        let mut project = project(true);
        let put = VisualOperation::ShapePut {
            sequence_id: "s".into(),
            shape: shape(),
        };
        assert_eq!(
            plan_apply(&project, &request(vec![put.clone()]))
                .unwrap_err()
                .code,
            "CONSTRAINT_VIOLATION"
        );
        project.base.sequences[0].tracks[0].locked = false;
        project = plan_apply(&project, &request(vec![put]))
            .unwrap()
            .proposed_project;
        project.base.sequences[0].tracks[0].locked = true;
        let effect: EffectNode = serde_json::from_value(json!({"id":"grade","target_id":"card",
            "effect":{"kind":"color_adjust","exposure_ev":0.0,"contrast":1.0,"saturation":1.0}}))
        .unwrap();
        assert_eq!(
            plan_apply(
                &project,
                &request(vec![VisualOperation::EffectPut {
                    sequence_id: "s".into(),
                    effect
                }])
            )
            .unwrap_err()
            .code,
            "CONSTRAINT_VIOLATION"
        );
        assert_eq!(
            plan_apply(
                &project,
                &request(vec![VisualOperation::ShapeDelete {
                    sequence_id: "s".into(),
                    id: "card".into()
                }])
            )
            .unwrap_err()
            .code,
            "CONSTRAINT_VIOLATION"
        );
    }
    #[test]
    fn delete_then_put_same_id_reports_final_change() {
        let original = plan_apply(
            &project(false),
            &request(vec![VisualOperation::ShapePut {
                sequence_id: "s".into(),
                shape: shape(),
            }]),
        )
        .unwrap()
        .proposed_project;
        let mut changed_shape = shape();
        changed_shape.opacity = 0.5;
        let plan = plan_apply(
            &original,
            &request(vec![
                VisualOperation::ShapeDelete {
                    sequence_id: "s".into(),
                    id: "card".into(),
                },
                VisualOperation::ShapePut {
                    sequence_id: "s".into(),
                    shape: changed_shape,
                },
            ]),
        )
        .unwrap();
        assert_eq!(plan.changed_ids, ["card"]);
        assert!(plan.created_ids.is_empty() && plan.deleted_ids.is_empty());
    }
}
