use super::*;
use serde_json::json;

pub(super) fn t(n: i64) -> Time {
    Time {
        ticks: n.to_string(),
        timebase: 1,
    }
}
pub(super) fn fixture() -> ProjectSnapshotV3 {
    let mut project=crate::project_v3::create(&serde_json::from_value(json!({"id":"p","name":"Editorial","sequence_id":"s","width":160,"height":90,"fps":{"numerator":10,"denominator":1}})).unwrap()).unwrap();
    project.base.assets = serde_json::from_value(json!([
        {"id":"video","name":"V","kind":"video","uri":"fixture.mp4","duration":t(100)},
        {"id":"sound","name":"A","kind":"audio","uri":"fixture.wav","duration":t(100)}
    ]))
    .unwrap();
    let clip = |id: &str, asset: &str, start: i64, duration: i64, source: i64| json!({"id":id,"asset_id":asset,"timeline_range":{"start":t(start),"duration":t(duration)},"source_range":{"start":t(source),"duration":t(duration)},"params":{},"channels":[]});
    project.base.sequences[0].tracks=serde_json::from_value(json!([
        {"id":"v","name":"Video","kind":"video","enabled":true,"muted":false,"locked":false,"clips":[clip("a","video",0,3,20),clip("b","video",3,3,30),clip("c","video",6,3,40)]},
        {"id":"a1","name":"Audio","kind":"audio","enabled":true,"muted":false,"locked":false,"clips":[clip("music","sound",0,9,10)]}
    ])).unwrap();
    project.base.sequences[0].caption_cues=serde_json::from_value(json!([{"id":"caption","text":"hello","timeline_range":{"start":t(1),"duration":t(6)},"source_anchor":null,"style":{"font_family":"Arial","font_size":16,"color":"#ffffff","background_color":null}}])).unwrap();
    project.base.sequences[0].tracks[0].clips[1].channels=serde_json::from_value(json!([{"parameter":"opacity","keyframes":[{"id":"key0","time":t(0),"value":0.0,"interpolation":"linear"},{"id":"key3","time":t(3),"value":1.0,"interpolation":"linear"}]}])).unwrap();
    project.keyframe_curves=serde_json::from_value(json!([{"id":"ease","schema_version":1,"sequence_id":"s","clip_id":"b","parameter":"opacity","keyframe_id":"key0","curve":{"x1":0.25,"y1":0.1,"x2":0.25,"y2":1.0}}])).unwrap();
    project.visual=serde_json::from_value(json!([{"sequence_id":"s","shapes":[],"effects":[{"id":"blur","target_id":"b","effect":{"kind":"gaussian_blur","radius_px":2.0}}],"masks":[{"id":"mask","target_id":"b","shape":{"kind":"rectangle","x":0.1,"y":0.1,"width":0.8,"height":0.8},"inverted":false,"feather":0.05}],"transitions":[]}])).unwrap();
    opencut_editor_core::project_v3::validate_project_v3(&project).unwrap();
    ProjectSnapshotV3 {
        project,
        revision: "rev_fixture".into(),
    }
}
fn request(command: TimelineCommand) -> TimelineEditRequest {
    TimelineEditRequest {
        project_id: "p".into(),
        expected_revision: "rev_fixture".into(),
        sequence_id: "s".into(),
        idempotency_key: "edit-1".into(),
        label: "Editorial change".into(),
        command,
    }
}
fn apply(
    snapshot: &ProjectSnapshotV3,
    command: TimelineCommand,
) -> (ProjectDocumentV3, TimelineEditPlan) {
    let plan = plan(snapshot, &request(command)).unwrap();
    let next = crate::project_v3::plan_apply(&snapshot.project, &plan.transaction)
        .unwrap()
        .proposed_project;
    (next, plan)
}
fn seconds(value: &Time) -> f64 {
    value.ticks.parse::<f64>().unwrap() / f64::from(value.timebase)
}
fn intervals(project: &ProjectDocumentV3, track: usize) -> Vec<(f64, f64, f64, f64)> {
    project.base.sequences[0].tracks[track]
        .clips
        .iter()
        .map(|c| {
            let s = c.source_range.as_ref().unwrap();
            (
                seconds(&c.timeline_range.start),
                seconds(&c.timeline_range.duration),
                seconds(&s.start),
                seconds(&s.duration),
            )
        })
        .collect()
}
#[test]
fn ripple_delete_preserves_retimed_sources_and_linked_visuals() {
    let original = fixture();
    let before = original.clone();
    let (next, receipt) = apply(
        &original,
        TimelineCommand::RippleDelete {
            range: TimeRange {
                start: t(2),
                duration: t(3),
            },
        },
    );
    assert_eq!(original, before);
    assert_eq!(
        intervals(&next, 0),
        vec![(0., 2., 20., 2.), (2., 1., 32., 1.), (3., 3., 40., 3.)]
    );
    assert_eq!(
        intervals(&next, 1),
        vec![(0., 2., 10., 2.), (2., 4., 15., 4.)]
    );
    assert_eq!(
        seconds(
            &next.base.sequences[0].caption_cues[0]
                .timeline_range
                .duration
        ),
        3.
    );
    let retained = &next.base.sequences[0].tracks[0].clips[1];
    assert!(
        next.visual[0]
            .masks
            .iter()
            .any(|m| m.target_id == retained.id)
    );
    assert!(
        next.visual[0]
            .effects
            .iter()
            .any(|m| m.target_id == retained.id)
    );
    assert!(
        next.keyframe_curves
            .iter()
            .any(|c| c.clip_id == retained.id)
    );
    assert!(receipt.deleted_ids.contains(&"b".to_owned()));
    let (again, other) = apply(
        &original,
        request(TimelineCommand::RippleDelete {
            range: TimeRange {
                start: t(2),
                duration: t(3),
            },
        })
        .command,
    );
    assert_eq!(next, again);
    assert_eq!(receipt, other);
}
#[test]
fn slip_changes_only_source_window() {
    let original = fixture();
    let (next, _) = apply(
        &original,
        TimelineCommand::Slip {
            clip_id: "b".into(),
            source_delta: t(2),
        },
    );
    let clip = &next.base.sequences[0].tracks[0].clips[1];
    assert_eq!(
        clip.timeline_range,
        original.project.base.sequences[0].tracks[0].clips[1].timeline_range
    );
    assert_eq!(
        clip.channels,
        original.project.base.sequences[0].tracks[0].clips[1].channels
    );
    assert_eq!(next.keyframe_curves, original.project.keyframe_curves);
    assert_eq!(seconds(&clip.source_range.as_ref().unwrap().start), 32.);
}
#[test]
fn roll_and_slide_preserve_outer_boundaries_and_source_speed() {
    let original = fixture();
    let (rolled, _) = apply(
        &original,
        TimelineCommand::Roll {
            outgoing_clip_id: "a".into(),
            incoming_clip_id: "b".into(),
            delta: t(1),
        },
    );
    assert_eq!(
        intervals(&rolled, 0),
        vec![(0., 4., 20., 4.), (4., 2., 31., 2.), (6., 3., 40., 3.)]
    );
    let (slid, _) = apply(
        &original,
        TimelineCommand::Slide {
            clip_id: "b".into(),
            delta: t(1),
        },
    );
    assert_eq!(
        intervals(&slid, 0),
        vec![(0., 4., 20., 4.), (4., 3., 30., 3.), (7., 2., 41., 2.)]
    );
    assert_eq!(
        slid.base.sequences[0].tracks[0].clips[1].channels,
        original.project.base.sequences[0].tracks[0].clips[1].channels
    );
}
#[test]
fn negative_roll_extends_source_and_shifts_keyframe_clock() {
    let (next, _) = apply(
        &fixture(),
        TimelineCommand::Roll {
            outgoing_clip_id: "a".into(),
            incoming_clip_id: "b".into(),
            delta: t(-1),
        },
    );
    assert_eq!(
        intervals(&next, 0),
        vec![(0., 2., 20., 2.), (2., 4., 29., 4.), (6., 3., 40., 3.)]
    );
    assert_eq!(
        seconds(&next.base.sequences[0].tracks[0].clips[1].channels[0].keyframes[0].time),
        1.
    );
}
#[test]
fn group_move_and_track_order_keep_stable_content_and_curves() {
    let original = fixture();
    let (moved, _) = apply(
        &original,
        TimelineCommand::MoveGroup {
            clip_ids: vec!["b".into(), "c".into(), "music".into()],
            caption_ids: vec!["caption".into()],
            shape_ids: vec![],
            delta: t(1),
        },
    );
    assert_eq!(
        seconds(&moved.base.sequences[0].caption_cues[0].timeline_range.start),
        2.
    );
    assert_eq!(
        seconds(
            &moved.base.sequences[0].tracks[1].clips[0]
                .timeline_range
                .start
        ),
        1.
    );
    let (reordered, _) = apply(
        &original,
        TimelineCommand::ReorderTracks {
            track_ids: vec!["a1".into(), "v".into()],
        },
    );
    assert_eq!(
        reordered.base.sequences[0].tracks[1],
        original.project.base.sequences[0].tracks[0]
    );
    assert_eq!(reordered.keyframe_curves, original.project.keyframe_curves);
    assert_eq!(reordered.visual, original.project.visual);
}
#[test]
fn locked_tracks_bounds_bad_selections_and_stale_revisions_reject_without_changes() {
    let mut original = fixture();
    original.project.base.sequences[0].tracks[1].locked = true;
    assert!(
        plan(
            &original,
            &request(TimelineCommand::RippleDelete {
                range: TimeRange {
                    start: t(2),
                    duration: t(1)
                }
            })
        )
        .is_err()
    );
    assert!(
        plan(
            &original,
            &request(TimelineCommand::Slip {
                clip_id: "b".into(),
                source_delta: t(-40)
            })
        )
        .is_err()
    );
    assert!(
        plan(
            &original,
            &request(TimelineCommand::ReorderTracks {
                track_ids: vec!["v".into(), "v".into()]
            })
        )
        .is_err()
    );
    assert!(
        plan(
            &original,
            &request(TimelineCommand::Roll {
                outgoing_clip_id: "a".into(),
                incoming_clip_id: "b".into(),
                delta: t(3)
            })
        )
        .is_err()
    );
    let mut stale = request(TimelineCommand::Slip {
        clip_id: "b".into(),
        source_delta: t(1),
    });
    stale.expected_revision = "stale".into();
    assert_eq!(
        plan(&original, &stale).unwrap_err().code,
        "REVISION_CONFLICT"
    );
    assert!(
        plan(
            &original,
            &request(TimelineCommand::Slip {
                clip_id: "b".into(),
                source_delta: t(1)
            })
        )
        .is_ok()
    );
}
#[test]
fn ripple_maps_non_unit_source_rate_exactly() {
    let mut original = fixture();
    original.project.base.sequences[0].tracks[1].clips[0]
        .source_range
        .as_mut()
        .unwrap()
        .duration = t(18);
    let (next, _) = apply(
        &original,
        TimelineCommand::RippleDelete {
            range: TimeRange {
                start: t(2),
                duration: t(3),
            },
        },
    );
    assert_eq!(
        intervals(&next, 1),
        vec![(0., 2., 10., 4.), (2., 4., 20., 8.)]
    );
}
#[test]
fn inserted_gap_splits_clips_and_preserves_animation_on_both_sides() {
    let (next, _) = apply(
        &fixture(),
        TimelineCommand::InsertGap {
            at: t(4),
            duration: t(2),
        },
    );
    assert_eq!(
        intervals(&next, 0),
        vec![
            (0., 3., 20., 3.),
            (3., 1., 30., 1.),
            (6., 2., 31., 2.),
            (8., 3., 40., 3.)
        ]
    );
    assert_eq!(next.keyframe_curves.len(), 2);
    assert_eq!(next.visual[0].masks.len(), 2);
}

#[test]
fn transitions_follow_retained_pieces_and_gap_removals_are_explicit() {
    let mut original = fixture();
    original.project.visual[0].transitions=serde_json::from_value(json!([{"id":"fade","outgoing_clip_id":"a","incoming_clip_id":"b","duration":t(1),"transition":{"kind":"crossfade"}}])).unwrap();
    let (rolled, receipt) = apply(
        &original,
        TimelineCommand::Roll {
            outgoing_clip_id: "a".into(),
            incoming_clip_id: "b".into(),
            delta: t(1),
        },
    );
    assert_eq!(
        rolled.visual[0].transitions[0].incoming_clip_id,
        rolled.base.sequences[0].tracks[0].clips[1].id
    );
    assert!(receipt.notes.is_empty());
    let (gap, receipt) = apply(
        &original,
        TimelineCommand::InsertGap {
            at: t(3),
            duration: t(1),
        },
    );
    assert!(gap.visual[0].transitions.is_empty());
    assert_eq!(receipt.notes.len(), 1);
    original.project.base.sequences[0].tracks[0].locked = true;
    let (moved, _) = apply(
        &original,
        TimelineCommand::MoveGroup {
            clip_ids: vec!["music".into()],
            caption_ids: vec![],
            shape_ids: vec![],
            delta: t(1),
        },
    );
    assert_eq!(
        moved.visual[0].transitions,
        original.project.visual[0].transitions
    );
}
#[test]
fn gap_splits_captions_shapes_and_their_masks_without_filling_the_gap() {
    let mut original = fixture();
    original.project.base.sequences[0].tracks.push(serde_json::from_value(json!({"id":"graphics","name":"Graphics","kind":"graphic","enabled":true,"muted":false,"locked":false,"clips":[]})).unwrap());
    original.project.visual[0].shapes=serde_json::from_value(json!([{"id":"shape","track_id":"graphics","timeline_range":{"start":t(1),"duration":t(6)},"primitive":{"kind":"rectangle","width_px":20,"height_px":20},"fill":{"red":255,"green":0,"blue":0,"alpha":255},"position_x":0,"position_y":0,"rotation_degrees":0,"opacity":1}])).unwrap();
    let (next, _) = apply(
        &original,
        TimelineCommand::InsertGap {
            at: t(4),
            duration: t(2),
        },
    );
    for r in next.base.sequences[0]
        .caption_cues
        .iter()
        .map(|c| &c.timeline_range)
        .chain(next.visual[0].shapes.iter().map(|s| &s.timeline_range))
    {
        assert!(seconds(&opencut_time::range_end(r).unwrap()) <= 4. || seconds(&r.start) >= 6.);
    }
    assert_eq!(next.base.sequences[0].caption_cues.len(), 2);
    assert_eq!(next.visual[0].shapes.len(), 2);
}
#[test]
fn malformed_time_and_out_of_bounds_selection_return_errors() {
    let original = fixture();
    let bad = Time {
        ticks: "1".into(),
        timebase: 0,
    };
    assert!(
        plan(
            &original,
            &request(TimelineCommand::InsertGap {
                at: bad,
                duration: t(1)
            })
        )
        .is_err()
    );
    assert!(
        plan(
            &original,
            &request(TimelineCommand::MoveGroup {
                clip_ids: vec!["a".into()],
                caption_ids: vec![],
                shape_ids: vec![],
                delta: t(-1)
            })
        )
        .is_err()
    );
    assert!(
        plan(
            &original,
            &request(TimelineCommand::MoveGroup {
                clip_ids: vec!["b".into()],
                caption_ids: vec![],
                shape_ids: vec![],
                delta: t(1)
            })
        )
        .is_err()
    );
    assert!(
        plan(
            &original,
            &request(TimelineCommand::Slip {
                clip_id: "b".into(),
                source_delta: t(80)
            })
        )
        .is_err()
    );
}
