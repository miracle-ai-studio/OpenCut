//! Shared media-library removal planning; persistence uses the existing edit authority.
use opencut_api_contracts::media_library::RemoveMedia;
use opencut_contracts::{
    EditorError, Operation, ProjectDocument, historical_visual::HistoricalVisualDocument,
    historical_visual_ops::VisualOperation,
};
use std::collections::HashSet;

pub fn validate(input: &RemoveMedia) -> Result<(), EditorError> {
    let mut ids = HashSet::new();
    if input.project_id.trim().is_empty()
        || input.expected_revision.trim().is_empty()
        || input.idempotency_key.trim().is_empty()
        || input.asset_ids.is_empty()
        || input.asset_ids.len() > 64
        || input
            .asset_ids
            .iter()
            .any(|id| id.trim().is_empty() || !ids.insert(id))
    {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Select between 1 and 64 media items.",
        ));
    }
    Ok(())
}
pub fn base_operations(
    document: &ProjectDocument,
    input: &RemoveMedia,
) -> Result<Vec<Operation>, EditorError> {
    validate(input)?;
    if document.id != input.project_id {
        return Err(EditorError::new(
            "INVALID_PROJECT",
            "Media removal scope differs from project",
        ));
    }
    for id in &input.asset_ids {
        if !document.assets.iter().any(|a| &a.id == id) {
            return Err(EditorError::new("NOT_FOUND", "Media item was not found"));
        }
    }
    let ids: HashSet<_> = input.asset_ids.iter().collect();
    let mut operations = Vec::new();
    for scene in &document.sequences {
        for track in &scene.tracks {
            for clip in &track.clips {
                if clip.asset_id.as_ref().is_some_and(|id| ids.contains(id)) {
                    operations.push(Operation::DeleteClip {
                        sequence_id: scene.id.clone(),
                        track_id: track.id.clone(),
                        clip_id: clip.id.clone(),
                    });
                }
            }
        }
        for cue in &scene.caption_cues {
            if cue
                .source_anchor
                .as_ref()
                .is_some_and(|anchor| ids.contains(&anchor.asset_id))
            {
                let mut cue = cue.clone();
                cue.source_anchor = None;
                operations.push(Operation::UpdateCaption {
                    sequence_id: scene.id.clone(),
                    cue,
                });
            }
        }
    }
    operations.extend(
        input
            .asset_ids
            .iter()
            .map(|asset_id| Operation::RemoveAsset {
                asset_id: asset_id.clone(),
            }),
    );
    limit_operations(operations.len())?;
    Ok(operations)
}
pub fn visual_operations(
    document: &HistoricalVisualDocument,
    input: &RemoveMedia,
) -> Result<Vec<VisualOperation>, EditorError> {
    let base = base_operations(&document.base, input)?;
    let mut operations = Vec::new();
    for graph in &document.visual {
        let clips: HashSet<_> = base
            .iter()
            .filter_map(|op| match op {
                Operation::DeleteClip {
                    sequence_id,
                    clip_id,
                    ..
                } if sequence_id == &graph.sequence_id => Some(clip_id),
                _ => None,
            })
            .collect();
        operations.extend(
            graph
                .effects
                .iter()
                .filter(|e| clips.contains(&e.target_id))
                .map(|e| VisualOperation::EffectDelete {
                    sequence_id: graph.sequence_id.clone(),
                    id: e.id.clone(),
                }),
        );
        operations.extend(
            graph
                .masks
                .iter()
                .filter(|m| clips.contains(&m.target_id))
                .map(|m| VisualOperation::MaskDelete {
                    sequence_id: graph.sequence_id.clone(),
                    id: m.id.clone(),
                }),
        );
        operations.extend(
            graph
                .transitions
                .iter()
                .filter(|t| {
                    clips.contains(&t.outgoing_clip_id) || clips.contains(&t.incoming_clip_id)
                })
                .map(|t| VisualOperation::TransitionDelete {
                    sequence_id: graph.sequence_id.clone(),
                    id: t.id.clone(),
                }),
        );
    }
    // Visual deletions must resolve their still-present targets for lock checks.
    operations.extend(
        base.into_iter()
            .map(|operation| VisualOperation::Base { operation }),
    );
    limit_operations(operations.len())?;
    Ok(operations)
}
fn limit_operations(count: usize) -> Result<(), EditorError> {
    if count > 128 {
        return Err(EditorError::new(
            "RESOURCE_TOO_LARGE",
            "Remove some linked clips before removing this media.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencut_contracts::{
        CreateProject, FrameRate, ProjectSnapshot, TransactionRequest,
        historical_visual_ops::ApplyHistoricalVisualProject,
    };
    use serde_json::json;
    fn fixture() -> HistoricalVisualDocument {
        let mut p = crate::historical_visual::create(&CreateProject {
            id: "p".into(),
            name: "P".into(),
            sequence_id: "main".into(),
            width: 640,
            height: 360,
            fps: FrameRate {
                numerator: 25,
                denominator: 1,
            },
        })
        .unwrap();
        for id in ["a", "b"] {
            p.base.assets.push(serde_json::from_value(json!({"id":id,"name":id,"kind":"video","uri":format!("/{id}.mp4"),"duration":{"ticks":"30","timebase":10},"metadata":{}})).unwrap());
        }
        let clip = |id: &str, asset: &str, start: &str| json!({"id":id,"asset_id":asset,"timeline_range":{"start":{"ticks":start,"timebase":10},"duration":{"ticks":"10","timebase":10}},"source_range":{"start":{"ticks":"5","timebase":10},"duration":{"ticks":"10","timebase":10}},"params":{},"channels":[]});
        p.base.sequences[0].tracks.push(serde_json::from_value(json!({"id":"video","name":"Video","kind":"video","enabled":true,"muted":false,"locked":false,"clips":[clip("a1","a","0"),clip("b1","b","10")]})).unwrap());
        let mut other = p.base.sequences[0].clone();
        other.id = "other".into();
        other.name = "Other".into();
        other.tracks[0].id = "other-video".into();
        other.tracks[0].clips = vec![serde_json::from_value(clip("a2", "a", "0")).unwrap()];
        p.base.sequences.push(other);
        p.base.sequences[0].caption_cues.push(serde_json::from_value(json!({"id":"caption","text":"Keep caption text","timeline_range":{"start":{"ticks":"0","timebase":10},"duration":{"ticks":"10","timebase":10}},"source_anchor":{"asset_id":"a","source_range":{"start":{"ticks":"0","timebase":10},"duration":{"ticks":"10","timebase":10}}},"style":{"font_family":"Arial","font_size":24,"color":"white","background_color":null}})).unwrap());
        p.visual.push(serde_json::from_value(json!({"sequence_id":"main","shapes":[],"effects":[{"id":"ea","target_id":"a1","effect":{"kind":"gaussian_blur","radius_px":3}},{"id":"eb","target_id":"b1","effect":{"kind":"gaussian_blur","radius_px":4}}],"masks":[{"id":"ma","target_id":"a1","shape":{"kind":"rectangle","x":0,"y":0,"width":1,"height":1},"inverted":false,"feather":0}],"transitions":[{"id":"ab","outgoing_clip_id":"a1","incoming_clip_id":"b1","duration":{"ticks":"2","timebase":10},"transition":{"kind":"crossfade"}}]})).unwrap());
        p
    }
    fn input() -> RemoveMedia {
        RemoveMedia {
            project_id: "p".into(),
            expected_revision: "head".into(),
            asset_ids: vec!["a".into()],
            idempotency_key: "remove".into(),
        }
    }
    #[test]
    fn both_formats_remove_all_references_preserve_unrelated_media_and_detach_captions() {
        let p = fixture();
        let input = input();
        let ops = visual_operations(&p, &input).unwrap();
        let plan = crate::historical_visual::plan_apply(
            &p,
            &ApplyHistoricalVisualProject {
                project_id: input.project_id.clone(),
                expected_revision: input.expected_revision.clone(),
                label: "Remove media".into(),
                idempotency_key: input.idempotency_key.clone(),
                operations: ops,
            },
        )
        .unwrap();
        let next = &plan.proposed_project;
        assert_eq!(
            next.base
                .assets
                .iter()
                .map(|a| a.id.as_str())
                .collect::<Vec<_>>(),
            vec!["b"]
        );
        assert_eq!(
            next.base.sequences[0].tracks[0]
                .clips
                .iter()
                .map(|c| c.id.as_str())
                .collect::<Vec<_>>(),
            vec!["b1"]
        );
        assert!(next.base.sequences[1].tracks[0].clips.is_empty());
        assert_eq!(next.visual[0].effects[0].id, "eb");
        assert_eq!(next.visual[0].effects.len(), 1);
        assert!(next.visual[0].masks.is_empty());
        assert!(next.visual[0].transitions.is_empty());
        assert_eq!(
            next.base.sequences[0].caption_cues[0].text,
            "Keep caption text"
        );
        assert!(
            next.base.sequences[0].caption_cues[0]
                .source_anchor
                .is_none()
        );
        let base = opencut_editor_core::plan_transaction(
            &ProjectSnapshot {
                project: p.base.clone(),
                revision: "head".into(),
            },
            &TransactionRequest {
                expected_revision: "head".into(),
                idempotency_key: "remove".into(),
                label: "Remove media".into(),
                operations: base_operations(&p.base, &input).unwrap(),
            },
        )
        .unwrap();
        assert_eq!(base.proposed_project, next.base);
    }
    #[test]
    fn a_locked_reference_rejects_the_whole_removal_and_invalid_selections_are_rejected() {
        let mut p = fixture();
        p.base.sequences[1].tracks[0].locked = true;
        let plan = crate::historical_visual::plan_apply(
            &p,
            &ApplyHistoricalVisualProject {
                project_id: "p".into(),
                expected_revision: "head".into(),
                label: "Remove media".into(),
                idempotency_key: "remove".into(),
                operations: visual_operations(&p, &input()).unwrap(),
            },
        );
        assert_eq!(plan.unwrap_err().code, "CONSTRAINT_VIOLATION");
        assert_eq!(p.base.assets.len(), 2);
        assert_eq!(p.visual[0].masks.len(), 1);
        let mut bad = input();
        bad.asset_ids.push("a".into());
        assert_eq!(validate(&bad).unwrap_err().code, "INVALID_ARGUMENT");
        bad.asset_ids = vec!["missing".into()];
        assert_eq!(
            base_operations(&p.base, &bad).unwrap_err().code,
            "NOT_FOUND"
        );
    }
}
