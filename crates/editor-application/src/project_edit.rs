//! Pure scene structure planning; persistence, authorization and journaling use the shared commit.
use opencut_contracts::{
    EditorError, Sequence, Track, TrackKind,
    historical_visual::HistoricalVisualSnapshot,
    historical_visual_ops::VisualPlan,
    project_edit::{ProjectEdit, ProjectEditRequest},
};
use std::collections::BTreeSet;

fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
fn name(value: &str) -> Result<String, EditorError> {
    if value.trim().is_empty() {
        return Err(invalid("A name must not be empty"));
    }
    Ok(value.trim().into())
}
pub fn plan(
    snapshot: &HistoricalVisualSnapshot,
    input: &ProjectEditRequest,
) -> Result<VisualPlan, EditorError> {
    opencut_editor_core::historical_visual::validate_project_historical_visual(&snapshot.project)?;
    if snapshot.project.base.id != input.project_id || snapshot.revision != input.expected_revision
    {
        return Err(EditorError::new(
            "REVISION_CONFLICT",
            "The project revision changed",
        ));
    }
    if input.idempotency_key.trim().is_empty()
        || input.label.trim().is_empty()
        || input.operations.is_empty()
        || input.operations.len() > 128
    {
        return Err(invalid(
            "A labeled, idempotent batch of 1 to 128 project edits is required",
        ));
    }
    let mut proposed = snapshot.project.clone();
    let mut created = BTreeSet::new();
    let mut changed = BTreeSet::new();
    let mut deleted = BTreeSet::new();
    for operation in &input.operations {
        match operation {
            ProjectEdit::RenameProject { name: value } => {
                proposed.base.name = name(value)?;
                changed.insert(input.project_id.clone());
            }
            ProjectEdit::CreateScene {
                sequence_id,
                name: value,
                video_track_id,
            } => {
                if sequence_id.trim().is_empty()
                    || video_track_id.trim().is_empty()
                    || proposed.base.sequences.iter().any(|s| s.id == *sequence_id)
                {
                    return Err(invalid("Scene and track IDs must be nonempty and unused"));
                }
                let main = proposed
                    .base
                    .sequences
                    .iter()
                    .find(|s| s.id == proposed.base.default_sequence_id)
                    .expect("validated main scene");
                let scene = Sequence {
                    id: sequence_id.clone(),
                    name: name(value)?,
                    width: main.width,
                    height: main.height,
                    fps: main.fps.clone(),
                    tracks: vec![Track {
                        id: video_track_id.clone(),
                        name: "Video".into(),
                        kind: TrackKind::Video,
                        enabled: true,
                        muted: false,
                        locked: false,
                        clips: vec![],
                        params: Default::default(),
                    }],
                    caption_cues: vec![],
                };
                proposed.base.sequences.push(scene);
                created.extend([sequence_id.clone(), video_track_id.clone()]);
            }
            ProjectEdit::RenameScene {
                sequence_id,
                name: value,
            } => {
                let scene = proposed
                    .base
                    .sequences
                    .iter_mut()
                    .find(|s| s.id == *sequence_id)
                    .ok_or_else(|| EditorError::new("NOT_FOUND", "Scene does not exist"))?;
                scene.name = name(value)?;
                changed.insert(sequence_id.clone());
            }
            ProjectEdit::DeleteScene { sequence_id } => {
                if proposed.base.default_sequence_id == *sequence_id {
                    return Err(EditorError::new(
                        "CONSTRAINT_VIOLATION",
                        "The main scene cannot be deleted",
                    ));
                }
                let index = proposed
                    .base
                    .sequences
                    .iter()
                    .position(|s| s.id == *sequence_id)
                    .ok_or_else(|| EditorError::new("NOT_FOUND", "Scene does not exist"))?;
                let scene = proposed.base.sequences.remove(index);
                deleted.insert(scene.id);
                for track in scene.tracks {
                    deleted.insert(track.id);
                    for clip in track.clips {
                        deleted.insert(clip.id);
                        for channel in clip.channels {
                            deleted.extend(channel.keyframes.into_iter().map(|k| k.id));
                        }
                    }
                }
                deleted.extend(scene.caption_cues.into_iter().map(|c| c.id));
                proposed.visual.retain(|v| {
                    if v.sequence_id != *sequence_id {
                        return true;
                    }
                    deleted.extend(v.shapes.iter().map(|s| s.id.clone()));
                    deleted.extend(v.effects.iter().map(|s| s.id.clone()));
                    deleted.extend(v.masks.iter().map(|s| s.id.clone()));
                    deleted.extend(v.transitions.iter().map(|s| s.id.clone()));
                    false
                });
                proposed.keyframe_curves.retain(|c| {
                    if c.sequence_id != *sequence_id {
                        return true;
                    }
                    deleted.insert(c.id.clone());
                    false
                });
            }
        }
        opencut_editor_core::historical_visual::validate_project_historical_visual(&proposed)?;
    }
    if proposed == snapshot.project {
        return Err(EditorError::new(
            "NO_EFFECT",
            "The project edit makes no changes",
        ));
    }
    Ok(VisualPlan {
        proposed_project: proposed,
        created_ids: created.into_iter().collect(),
        changed_ids: changed.into_iter().collect(),
        deleted_ids: deleted.into_iter().collect(),
    })
}
