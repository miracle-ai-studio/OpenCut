use super::super::tests::{fixture, t};
use super::*;
use serde_json::json;

fn request(command: ClassicCommandV1, ripple: bool) -> ClassicEditRequestV1 {
    ClassicEditRequestV1 {
        project_id: "p".into(),
        expected_revision: "rev_fixture".into(),
        sequence_id: "s".into(),
        idempotency_key: "classic-1".into(),
        label: "Classic action".into(),
        command,
        ripple,
    }
}
fn split(retain: SplitRetainV1) -> ClassicCommandV1 {
    ClassicCommandV1::Split {
        clip_ids: vec!["b".into()],
        caption_ids: vec![],
        shape_ids: vec![],
        at: t(4),
        retain,
    }
}
fn apply(
    snapshot: &ProjectSnapshotV3,
    command: ClassicCommandV1,
    ripple: bool,
) -> (ProjectDocumentV3, ClassicEditPlanV1) {
    let result = plan(snapshot, &request(command, ripple)).unwrap();
    let next = crate::project_v3::plan_apply(&snapshot.project, &result.plan.transaction)
        .unwrap()
        .proposed_project;
    (next, result)
}
#[test]
fn retained_sides_preserve_exact_source_animation_effects_and_track_local_ripple() {
    for retain in [
        SplitRetainV1::Both,
        SplitRetainV1::Left,
        SplitRetainV1::Right,
    ] {
        for ripple in [false, true] {
            let snapshot = fixture();
            let untouched = snapshot.clone();
            let (next, result) = apply(&snapshot, split(retain), ripple);
            assert_eq!(snapshot, untouched);
            assert_eq!(
                next.base.sequences[0].tracks[1],
                snapshot.project.base.sequences[0].tracks[1]
            );
            assert_eq!(
                next.base.sequences[0].caption_cues,
                snapshot.project.base.sequences[0].caption_cues
            );
            let clips = &next.base.sequences[0].tracks[0].clips;
            let later = clips.iter().find(|c| c.id == "c").unwrap();
            assert_eq!(
                later.timeline_range.start,
                t(if ripple {
                    match retain {
                        SplitRetainV1::Left => 4,
                        SplitRetainV1::Right => 5,
                        _ => 6,
                    }
                } else {
                    6
                })
            );
            if retain != SplitRetainV1::Left {
                let right_id = &result.selection.as_ref().unwrap().clip_ids[0];
                let right = clips.iter().find(|c| &c.id == right_id).unwrap();
                assert_eq!(right.source_range.as_ref().unwrap().start, t(31));
                assert_eq!(right.source_range.as_ref().unwrap().duration, t(2));
                assert_eq!(
                    right.timeline_range.start,
                    t(if ripple && retain == SplitRetainV1::Right {
                        3
                    } else {
                        4
                    })
                );
                assert_eq!(right.channels[0].keyframes[0].time, t(0));
                assert!(next.keyframe_curves.iter().any(|c| &c.clip_id == right_id));
                assert!(
                    next.visual[0]
                        .effects
                        .iter()
                        .any(|e| &e.target_id == right_id)
                );
                assert!(
                    next.visual[0]
                        .masks
                        .iter()
                        .any(|e| &e.target_id == right_id)
                );
                assert_eq!(
                    result.selection.as_ref().unwrap().playhead,
                    if ripple && retain == SplitRetainV1::Right {
                        Some(t(3))
                    } else {
                        None
                    }
                );
            }
        }
    }
}
#[test]
fn boundaries_locks_missing_ids_duplicate_selection_and_fixed_revision_reject_without_mutating() {
    let mut snapshot = fixture();
    let original = snapshot.clone();
    for at in [3, 6, 9] {
        let mut command = split(SplitRetainV1::Right);
        if let ClassicCommandV1::Split { at: time, .. } = &mut command {
            *time = t(at);
        }
        assert_eq!(
            plan(&snapshot, &request(command, true)).unwrap_err().code,
            "NO_EFFECT"
        );
    }
    snapshot.project.base.sequences[0].tracks[0].locked = true;
    assert_eq!(
        plan(&snapshot, &request(split(SplitRetainV1::Both), true))
            .unwrap_err()
            .code,
        "CONSTRAINT_VIOLATION"
    );
    snapshot = original.clone();
    let mut input = request(split(SplitRetainV1::Both), true);
    input.expected_revision = "old".into();
    assert_eq!(
        plan(&snapshot, &input).unwrap_err().code,
        "REVISION_CONFLICT"
    );
    if let ClassicCommandV1::Split { clip_ids, .. } = &mut input.command {
        *clip_ids = vec!["b".into(), "b".into()];
    }
    input.expected_revision = snapshot.revision.clone();
    assert_eq!(
        plan(&snapshot, &input).unwrap_err().code,
        "INVALID_ARGUMENT"
    );
    if let ClassicCommandV1::Split { clip_ids, .. } = &mut input.command {
        *clip_ids = vec!["missing".into()];
    }
    assert_eq!(plan(&snapshot, &input).unwrap_err().code, "NOT_FOUND");
    assert_eq!(snapshot, original);
}
#[test]
fn multiple_selected_clips_close_only_vacated_union_and_cross_track_move_does_not_close_source() {
    let snapshot = fixture();
    let ops = vec![VisualOperation::Base {
        operation: Operation::DeleteClip {
            sequence_id: "s".into(),
            track_id: "v".into(),
            clip_id: "a".into(),
        },
    }];
    let (next, _) = apply(&snapshot, ClassicCommandV1::Apply { operations: ops }, true);
    assert_eq!(
        next.base.sequences[0].tracks[0].clips[0]
            .timeline_range
            .start,
        t(0)
    );
    assert_eq!(
        next.base.sequences[0].tracks[0].clips[1]
            .timeline_range
            .start,
        t(3)
    );
    assert_eq!(
        next.base.sequences[0].tracks[1],
        snapshot.project.base.sequences[0].tracks[1]
    );
    let mut modified = snapshot.clone();
    let mut destination = modified.project.base.sequences[0].tracks[0].clone();
    destination.id = "v2".into();
    destination.clips.clear();
    modified.project.base.sequences[0].tracks.push(destination);
    let (moved, _) = apply(
        &modified,
        ClassicCommandV1::Apply {
            operations: vec![VisualOperation::Base {
                operation: Operation::MoveClip {
                    sequence_id: "s".into(),
                    track_id: "v".into(),
                    clip_id: "a".into(),
                    destination_track_id: "v2".into(),
                    start: t(0),
                },
            }],
        },
        true,
    );
    assert_eq!(
        moved.base.sequences[0].tracks[0].clips[0]
            .timeline_range
            .start,
        t(3)
    );
}
#[test]
fn caption_shape_split_preserves_related_nodes_and_row_isolation() {
    let mut snapshot = fixture();
    snapshot.project.base.sequences[0].tracks.push(serde_json::from_value(json!({"id":"g","name":"G","kind":"graphic","enabled":true,"muted":false,"locked":false,"clips":[]})).unwrap());
    snapshot.project.visual[0].shapes=serde_json::from_value(json!([{"id":"shape","track_id":"g","timeline_range":{"start":t(1),"duration":t(6)},"primitive":{"kind":"rectangle","width_px":10,"height_px":10},"fill":{"red":255,"green":0,"blue":0,"alpha":255},"position_x":0,"position_y":0,"rotation_degrees":0,"opacity":1}])).unwrap();
    let (next, result) = apply(
        &snapshot,
        ClassicCommandV1::Split {
            clip_ids: vec![],
            caption_ids: vec!["caption".into()],
            shape_ids: vec!["shape".into()],
            at: t(4),
            retain: SplitRetainV1::Right,
        },
        true,
    );
    let selected = result.selection.unwrap();
    let cue = &next.base.sequences[0].caption_cues[0];
    assert_eq!(cue.id, selected.caption_ids[0]);
    assert_eq!(cue.timeline_range.start, t(1));
    assert_eq!(cue.timeline_range.duration, t(3));
    let shape = &next.visual[0].shapes[0];
    assert_eq!(shape.id, selected.shape_ids[0]);
    assert_eq!(shape.timeline_range.start, t(1));
    assert_eq!(
        next.base.sequences[0].tracks[0],
        snapshot.project.base.sequences[0].tracks[0]
    );
}
#[test]
fn freed_interval_subtraction_is_exact_for_mixed_timebases_and_overlap() {
    let span = |id: &str, a: R, b: R| Span {
        id: id.into(),
        start: a,
        finish: b,
        target: Target::Clip(id.into()),
    };
    let one = R::new(1001, 30000).unwrap();
    let two = R::new(1001, 15000).unwrap();
    let three = R::new(1001, 10000).unwrap();
    let values = freed(
        &[span("old", one, three)],
        &[span("new", two, three)],
        &BTreeSet::from(["new".into()]),
    );
    assert_eq!(values, vec![(one, two)]);
}

#[test]
fn wrapped_base_operations_cannot_escape_the_captured_scene() {
    let mut snapshot = fixture();
    let mut other = snapshot.project.base.sequences[0].clone();
    other.id = "other".into();
    other.tracks.clear();
    other.caption_cues.clear();
    snapshot.project.base.sequences.push(other);
    let input = ClassicEditRequestV1 {
        project_id: "p".into(),
        expected_revision: snapshot.revision.clone(),
        sequence_id: "s".into(),
        idempotency_key: "scope".into(),
        label: "Scope".into(),
        ripple: true,
        command: ClassicCommandV1::Apply {
            operations: vec![VisualOperation::Base {
                operation: Operation::UpdateSequenceSettings {
                    sequence_id: "other".into(),
                    name: None,
                    width: None,
                    height: None,
                    fps: Some(opencut_contracts::FrameRate {
                        numerator: 60,
                        denominator: 1,
                    }),
                },
            }],
        },
    };
    assert_eq!(
        plan(&snapshot, &input).unwrap_err().code,
        "INVALID_ARGUMENT"
    );
    assert_eq!(snapshot.project.base.sequences[1].fps.numerator, 10);
}
