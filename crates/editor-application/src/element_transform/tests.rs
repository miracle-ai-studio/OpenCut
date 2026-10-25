use super::*;
use crate::scene::{SceneRead, StoredProjectRevision};
use opencut_contracts::{
    AssetVersion, CreateProject, FrameRate, TransactionResult,
    scene_ops::{CopySceneProject, CreateProjectRequest},
};
use serde_json::json;
use std::cell::Cell;

mod geometry;
mod visual_draft;
fn t(n: i64) -> Time {
    Time {
        ticks: n.to_string(),
        timebase: 1,
    }
}
struct Store {
    project: EditorDocument,
    reads: Cell<usize>,
    authorizations: Cell<usize>,
    advance_at: Option<usize>,
    deny_at: Option<usize>,
    wrong_scope: bool,
}
impl Store {
    fn new() -> Self {
        let mut project = crate::scene::new_document(&CreateProject {
            id: "project".into(),
            name: "Transforms".into(),
            sequence_id: "scene".into(),
            width: 320,
            height: 180,
            fps: FrameRate {
                numerator: 30,
                denominator: 1,
            },
        })
        .unwrap();
        let mut track = project.sequences[0].tracks[0].clone();
        track.id = "graphics".into();
        track.kind = SceneTrackKind::Graphic;
        track.elements.push(serde_json::from_value(json!({
            "id":"first","name":"Graphic","enabled":true,
            "timeline_range":{"start":t(1),"duration":t(4)},
            "source_span":{"profile":"classic_trimmed","trim_start":t(0),"trim_end":t(0),"source_duration":null},
            "content":{"kind":"graphic","graphic":{"source_size":{"width":100,"height":100},"fill":{"encoding":"srgb","red":1,"green":0,"blue":0,"alpha":1},"stroke":{"color":{"encoding":"srgb","red":0,"green":0,"blue":0,"alpha":1},"width_px":0,"align":"center"},"geometry":{"kind":"rectangle","corner_radius_percent":0}}},
            "visual":{"profile":"classic_canvas_v31","transform":{"position_x_px":7,"position_y_px":-3,"scale_x":1,"scale_y":2,"rotation_degrees":11},"opacity":1,"blend_mode":"normal","effects":[],"masks":[]},"channels":[]
        })).unwrap());
        let mut second = track.elements[0].clone();
        second.id = "second".into();
        second.visual.as_mut().unwrap().transform.position_x_px = 50.;
        let mut second_track = track.clone();
        second_track.id = "graphics-second".into();
        second_track.elements = vec![second];
        project.sequences[0].tracks.push(track);
        project.sequences[0].tracks.push(second_track);
        Self {
            project,
            reads: Cell::new(0),
            authorizations: Cell::new(0),
            advance_at: None,
            deny_at: None,
            wrong_scope: false,
        }
    }
    fn element(&self) -> &TimelineElement {
        &self.project.sequences[0].tracks[1].elements[0]
    }
    fn element_mut(&mut self) -> &mut TimelineElement {
        &mut self.project.sequences[0].tracks[1].elements[0]
    }
    fn sample(&self) -> SampleElementTransforms {
        SampleElementTransforms {
            project_id: "project".into(),
            expected_revision: "head".into(),
            sequence_id: "scene".into(),
            time: t(2),
            element_ids: vec!["first".into()],
        }
    }
    fn tx(&self, edit: impl FnOnce(&mut TimelineElement)) -> ApplyProject {
        let mut element = self.element().clone();
        edit(&mut element);
        ApplyProject {
            branch_name: None,
            project_id: "project".into(),
            expected_revision: "head".into(),
            idempotency_key: "gesture-original".into(),
            label: "Transform selection".into(),
            operations: vec![ProjectEditOperation::ElementPut {
                sequence_id: "scene".into(),
                track_id: "graphics".into(),
                element: Box::new(element),
            }],
        }
    }
}
impl SceneRead for Store {
    fn authorize_project(&self, intent: SceneCommitIntent<'_>) -> Result<(), EditorError> {
        assert!(matches!(intent, SceneCommitIntent::PublicApply(_)));
        let n = self.authorizations.get() + 1;
        self.authorizations.set(n);
        if self.deny_at.is_some_and(|at| n >= at) {
            Err(EditorError::new("NOT_AUTHORIZED", "revoked"))
        } else {
            Ok(())
        }
    }
    fn head_scene_source(&self, _: &str) -> Result<StoredProjectRevision, EditorError> {
        let n = self.reads.get() + 1;
        self.reads.set(n);
        let mut project = self.project.clone();
        if self.wrong_scope {
            project.id = "other".into();
        }
        Ok(StoredProjectRevision {
            revision: if self.advance_at.is_some_and(|at| n >= at) {
                "advanced"
            } else {
                "head"
            }
            .into(),
            document: StoredProjectDocument::Editable(Box::new(project)),
        })
    }
    fn revision_scene_source(
        &self,
        _: &str,
        _: &str,
    ) -> Result<StoredProjectRevision, EditorError> {
        unreachable!("No historical fallback")
    }
    fn replay_scene_create(
        &self,
        _: &CreateProjectRequest,
    ) -> Result<Option<EditorSnapshot>, EditorError> {
        unreachable!("No write")
    }
    fn replay_scene_copy(
        &self,
        _: &CopySceneProject,
    ) -> Result<Option<EditorSnapshot>, EditorError> {
        unreachable!("No write")
    }
    fn replay_scene_edit(
        &self,
        _: SceneCommitIntent<'_>,
    ) -> Result<Option<TransactionResult>, EditorError> {
        unreachable!("No receipt")
    }
}
impl ProjectPublicResources for Store {
    fn authorized_project_media_scene(
        &self,
        _: &str,
        _: &str,
        _: &AssetVersion,
    ) -> Result<ProjectAsset, EditorError> {
        unreachable!("No media reads")
    }
}
fn channel(
    element: &str,
    id: &str,
    parameter: ScalarParameter,
    a: f64,
    b: f64,
) -> AnimationChannel {
    AnimationChannel::Scalar {
        id: id.into(),
        target: ChannelTarget::Element {
            element_id: element.into(),
        },
        binding: ScalarBinding::Property { parameter },
        keys: vec![
            ScalarKey {
                id: format!("{id}-left"),
                segment_id: None,
                time: t(0),
                value: a,
                left_handle: None,
                right_handle: Some(CurveHandle {
                    dt: Time {
                        ticks: "1".into(),
                        timebase: 2,
                    },
                    dv: (b - a) / 4.,
                }),
                segment_to_next: ScalarSegment::Bezier,
                tangent_mode: TangentMode::Broken,
            },
            ScalarKey {
                id: format!("{id}-right"),
                segment_id: None,
                time: t(2),
                value: b,
                left_handle: Some(CurveHandle {
                    dt: Time {
                        ticks: "-1".into(),
                        timebase: 2,
                    },
                    dv: -(b - a) / 4.,
                }),
                right_handle: None,
                segment_to_next: ScalarSegment::Linear,
                tangent_mode: TangentMode::Broken,
            },
        ],
        extrapolation: Extrapolation {
            before: ExtrapolationMode::Hold,
            after: ExtrapolationMode::Hold,
        },
    }
}
fn candidate_element(snapshot: &EditorSnapshot) -> &TimelineElement {
    &snapshot.project.sequences[0].tracks[1].elements[0]
}
fn strip(element: &mut TimelineElement, intent: TransformGestureIntent) {
    element
        .channels
        .retain(|channel| !removed_axis(parameter(channel, &element.id), intent));
}
#[test]
fn sample_uses_exact_element_local_time_and_preserves_requested_order_without_io() {
    let mut store = Store::new();
    store.element_mut().channels = vec![
        channel("first", "x", ScalarParameter::ScaleX, 1., 3.),
        channel(
            "first",
            "rotation",
            ScalarParameter::RotationDegrees,
            10.,
            30.,
        ),
    ];
    let mut input = store.sample();
    input.element_ids = vec!["second".into(), "first".into()];
    let before = store.project.clone();
    let result = sample(&store, &input).unwrap();
    assert_eq!(result.time, input.time);
    assert_eq!(result.revision, "head");
    assert_eq!(
        result
            .elements
            .iter()
            .map(|e| e.element_id.as_str())
            .collect::<Vec<_>>(),
        vec!["second", "first"]
    );
    assert_eq!(result.elements[1].track_id, "graphics");
    assert!((result.elements[1].transform.scale_x - 2.).abs() < 1e-9);
    assert!((result.elements[1].transform.rotation_degrees - 20.).abs() < 1e-9);
    assert_eq!(store.project, before);
    assert_eq!((store.reads.get(), store.authorizations.get()), (2, 2));
}
#[test]
fn scale_removes_only_requested_axis_and_preserves_every_other_curve_and_authored_base() {
    for intent in [
        TransformGestureIntent::ScaleX,
        TransformGestureIntent::ScaleY,
        TransformGestureIntent::ScaleXy,
    ] {
        let mut store = Store::new();
        store.element_mut().channels = vec![
            channel("first", "x", ScalarParameter::ScaleX, 1., 3.),
            channel("first", "y", ScalarParameter::ScaleY, 2., 6.),
            channel("first", "position", ScalarParameter::PositionXPx, 10., 50.),
            channel(
                "first",
                "rotation",
                ScalarParameter::RotationDegrees,
                10.,
                90.,
            ),
            channel("first", "opacity", ScalarParameter::Opacity, 0.2, 0.8),
        ];
        let before = store.project.clone();
        let tx = store.tx(|e| {
            let v = &mut e.visual.as_mut().unwrap().transform;
            match intent {
                TransformGestureIntent::ScaleX => v.scale_x = 4.,
                TransformGestureIntent::ScaleY => v.scale_y = 8.,
                _ => {
                    v.scale_x = 4.;
                    v.scale_y = 8.;
                }
            }
            strip(e, intent);
        });
        let (candidate, _) = prepare_draft(&store, &tx, "scene", &t(2), intent).unwrap();
        let expected = store
            .element()
            .channels
            .iter()
            .filter(|c| !removed_axis(parameter(c, "first"), intent))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(candidate_element(&candidate).channels, expected);
        assert_eq!(
            candidate_element(&candidate)
                .visual
                .as_ref()
                .unwrap()
                .transform
                .position_x_px,
            7.
        );
        assert_eq!(
            candidate_element(&candidate)
                .visual
                .as_ref()
                .unwrap()
                .transform
                .rotation_degrees,
            11.
        );
        assert_eq!(store.project, before);
    }
}
#[test]
fn translate_and_rotate_reject_only_their_own_animation_or_moving_animated_pivot() {
    let mut store = Store::new();
    store.element_mut().channels = vec![channel(
        "first",
        "rotation",
        ScalarParameter::RotationDegrees,
        0.,
        90.,
    )];
    let translate = store.tx(|e| e.visual.as_mut().unwrap().transform.position_x_px += 5.);
    prepare_draft(
        &store,
        &translate,
        "scene",
        &t(2),
        TransformGestureIntent::Translate,
    )
    .unwrap();
    let rotate = store.tx(|e| e.visual.as_mut().unwrap().transform.rotation_degrees += 20.);
    assert!(
        prepare_draft(
            &store,
            &rotate,
            "scene",
            &t(2),
            TransformGestureIntent::Rotate
        )
        .is_err()
    );
    store.element_mut().channels = vec![channel(
        "first",
        "position",
        ScalarParameter::PositionXPx,
        0.,
        90.,
    )];
    let rotate = store.tx(|e| e.visual.as_mut().unwrap().transform.rotation_degrees += 20.);
    prepare_draft(
        &store,
        &rotate,
        "scene",
        &t(2),
        TransformGestureIntent::Rotate,
    )
    .unwrap();
    for intent in [
        TransformGestureIntent::Translate,
        TransformGestureIntent::Rotate,
        TransformGestureIntent::ScaleX,
    ] {
        let moved = store.tx(|e| e.visual.as_mut().unwrap().transform.position_x_px += 5.);
        assert!(prepare_draft(&store, &moved, "scene", &t(2), intent).is_err());
    }
}
#[test]
fn rejects_unrelated_base_changes_curve_edits_missing_axis_removal_and_foreign_operations() {
    let mut store = Store::new();
    store.element_mut().channels = vec![
        channel("first", "x", ScalarParameter::ScaleX, 1., 3.),
        channel("first", "opacity", ScalarParameter::Opacity, 0.2, 0.8),
    ];
    for tamper in 0..6 {
        let tx = store.tx(|e| {
            e.visual.as_mut().unwrap().transform.scale_x = 4.;
            if tamper != 0 {
                strip(e, TransformGestureIntent::ScaleX);
            }
            match tamper {
                1 => e.visual.as_mut().unwrap().transform.scale_y = 9.,
                2 => e.visual.as_mut().unwrap().transform.rotation_degrees = 90.,
                3 => e.name = "forged".into(),
                4 => e.channels.clear(),
                5 => {
                    let AnimationChannel::Scalar { keys, .. } = &mut e.channels[0] else {
                        panic!()
                    };
                    keys[0].right_handle = None;
                }
                _ => {}
            }
        });
        assert!(
            prepare_draft(&store, &tx, "scene", &t(2), TransformGestureIntent::ScaleX).is_err(),
            "tamper {tamper}"
        );
    }
    let valid = store.tx(|e| {
        e.visual.as_mut().unwrap().transform.scale_x = 4.;
        strip(e, TransformGestureIntent::ScaleX);
    });
    let mut duplicate = valid.clone();
    duplicate.operations.push(duplicate.operations[0].clone());
    assert!(
        prepare_draft(
            &store,
            &duplicate,
            "scene",
            &t(2),
            TransformGestureIntent::ScaleX
        )
        .is_err()
    );
    let mut foreign = valid.clone();
    let ProjectEditOperation::ElementPut { track_id, .. } = &mut foreign.operations[0] else {
        panic!()
    };
    *track_id = "other".into();
    assert!(
        prepare_draft(
            &store,
            &foreign,
            "scene",
            &t(2),
            TransformGestureIntent::ScaleX
        )
        .is_err()
    );
    let mut rename = valid;
    rename.operations = vec![ProjectEditOperation::Rename {
        name: "forged".into(),
    }];
    assert!(
        prepare_draft(
            &store,
            &rename,
            "scene",
            &t(2),
            TransformGestureIntent::ScaleX
        )
        .is_err()
    );
}
#[test]
fn scope_activity_bounds_authority_and_final_head_fences_reject_stale_samples_and_drafts() {
    for case in 0..8 {
        let mut store = Store::new();
        let mut sample = store.sample();
        match case {
            0 => sample.time = t(5),
            1 => store.project.sequences[0].tracks[1].locked = true,
            2 => store.element_mut().enabled = false,
            3 => store.project.sequences[0].tracks[1].enabled = false,
            4 => sample.expected_revision = "historical".into(),
            5 => store.wrong_scope = true,
            6 => store.advance_at = Some(2),
            _ => store.deny_at = Some(2),
        }
        assert!(super::sample(&store, &sample).is_err(), "case {case}");
    }
    for case in 0..4 {
        let mut store = Store::new();
        let tx = store.tx(|e| e.visual.as_mut().unwrap().transform.scale_x = 2.);
        match case {
            0 => store.advance_at = Some(2),
            1 => store.advance_at = Some(3),
            2 => store.deny_at = Some(2),
            _ => store.deny_at = Some(3),
        }
        assert!(
            prepare_draft(&store, &tx, "scene", &t(2), TransformGestureIntent::ScaleX).is_err()
        );
    }
    let store = Store::new();
    for ids in [
        vec![],
        vec!["first".into(); 2],
        (0..61).map(|n| format!("element-{n}")).collect(),
    ] {
        let mut sample = store.sample();
        sample.element_ids = ids;
        assert!(super::sample(&store, &sample).is_err());
    }
}
#[test]
fn multi_element_proposal_is_one_public_transaction_and_digest_binds_intent() {
    let store = Store::new();
    let mut tx = store.tx(|e| e.visual.as_mut().unwrap().transform.position_x_px += 10.);
    let mut second = store.project.sequences[0].tracks[2].elements[0].clone();
    second.visual.as_mut().unwrap().transform.position_x_px += 10.;
    tx.operations.push(ProjectEditOperation::ElementPut {
        sequence_id: "scene".into(),
        track_id: "graphics-second".into(),
        element: Box::new(second),
    });
    let (candidate, translate) = prepare_draft(
        &store,
        &tx,
        "scene",
        &t(2),
        TransformGestureIntent::Translate,
    )
    .unwrap();
    assert_eq!(
        candidate_element(&candidate)
            .visual
            .as_ref()
            .unwrap()
            .transform
            .position_x_px,
        17.
    );
    assert_eq!(
        candidate.project.sequences[0].tracks[2].elements[0]
            .visual
            .as_ref()
            .unwrap()
            .transform
            .position_x_px,
        60.
    );
    let (_, rotate) =
        prepare_draft(&store, &tx, "scene", &t(2), TransformGestureIntent::Rotate).unwrap();
    assert_ne!(translate, rotate);
    assert_eq!(
        serde_json::to_string(&TransformGestureIntent::ScaleXy).unwrap(),
        "\"scale_xy\""
    );
}

#[test]
fn transitions_are_rejected_even_outside_the_transition_window() {
    let mut store = Store::new();
    let second = store.project.sequences[0]
        .tracks
        .remove(2)
        .elements
        .remove(0);
    store.project.sequences[0].tracks[1].elements.push(second);
    store.project.assets.push(serde_json::from_value(json!({"id":"asset","name":"Video","kind":"video","version":{"id":format!("av_{}","a".repeat(64)),"content_hash":"a".repeat(64),"size":1},"duration":t(12),"origin":{"kind":"legacy_locator","uri":"opencut-media://offline"},"metadata":{"mime_type":null,"intrinsic_size":null,"frame_rate":null,"rotation_degrees":null,"audio":{"status":"absent"}},"license":null})).unwrap());
    let track = &mut store.project.sequences[0].tracks[1];
    track.kind = SceneTrackKind::Video;
    for (index, element) in track.elements.iter_mut().enumerate() {
        element.timeline_range.start = t(if index == 0 { 1 } else { 5 });
        element.source_span = ElementSourceSpan::ClassicTrimmed {
            trim_start: t(1),
            trim_end: t(7),
            source_duration: Some(t(12)),
        };
        element.content=serde_json::from_value(json!({"kind":"video","asset_id":"asset","video_stream":null,"source_audio_enabled":true,"audio":{"stream":null,"gain":{"unit":"decibels","value":0},"muted":false},"retime":{"rate":{"numerator":1,"denominator":1},"maintain_pitch":false,"mapping":{"kind":"exact_rational"}}})).unwrap();
    }
    store.project.sequences[0]
        .transitions
        .push(SceneTransition {
            id: "crossfade".into(),
            outgoing_element_id: "first".into(),
            incoming_element_id: "second".into(),
            duration: t(1),
            transition: TransitionKind::LegacyCrossfadeV3,
        });
    opencut_editor_core::scene::validate_project_scene(&store.project).unwrap();
    assert!(
        sample(&store, &store.sample())
            .unwrap_err()
            .message
            .contains("transition")
    );
    let tx = store.tx(|e| e.visual.as_mut().unwrap().transform.scale_x = 2.);
    assert!(
        prepare_draft(&store, &tx, "scene", &t(2), TransformGestureIntent::ScaleX)
            .unwrap_err()
            .message
            .contains("transition")
    );
    // Static properties are composited by the original transition IR and do
    // not need the gesture's unambiguous geometry restriction.
    let tx = store.tx(|e| e.visual.as_mut().unwrap().opacity = 0.75);
    let (candidate, _) = crate::visual_edit_draft::prepare(&store, &tx, "scene", &t(2),
        opencut_contracts::visual_edit_draft::VisualEditDraftIntent::Property {
            property: opencut_contracts::visual_edit_draft::VisualProperty::Opacity,
        }).unwrap();
    assert_eq!(candidate.project.sequences[0].transitions, store.project.sequences[0].transitions);
}
