use super::*;
use crate::scene::{SceneRead, StoredProjectRevision};
use opencut_contracts::{
    AssetVersion, CreateProject, FrameRate, TransactionResult,
    scene_ops::{CopySceneProject, CreateProjectRequest},
};
use std::cell::Cell;
fn t(n: i64) -> Time {
    Time {
        ticks: n.to_string(),
        timebase: 1,
    }
}
fn key(id: &str, time: i64, value: f64) -> ScalarKey {
    ScalarKey {
        id: id.into(),
        segment_id: None,
        time: t(time),
        value,
        left_handle: None,
        right_handle: None,
        segment_to_next: ScalarSegment::Linear,
        tangent_mode: TangentMode::Broken,
    }
}
fn channel() -> AnimationChannel {
    AnimationChannel::Scalar {
        id: "animation".into(),
        target: ChannelTarget::Element {
            element_id: "target".into(),
        },
        binding: ScalarBinding::Property {
            parameter: ScalarParameter::PositionXPx,
        },
        keys: vec![key("left", 0, 0.), key("right", 2, 100.)],
        extrapolation: Extrapolation {
            before: ExtrapolationMode::Hold,
            after: ExtrapolationMode::Hold,
        },
    }
}

#[test]
fn legacy_conversion_is_a_read_only_fixed_head_public_plan_and_rechecks_authority() {
    use opencut_contracts::keyframe_legacy_conversion::*;
    let mut store = Store::new();
    let AnimationChannel::Scalar { keys, .. } =
        &mut store.project.sequences[0].tracks[0].elements[0].channels[0]
    else {
        panic!()
    };
    keys[0].segment_to_next = ScalarSegment::LegacyNormalizedBezier {
        curve: opencut_contracts::historical_visual::HistoricalVisualCubicBezier {
            x1: 0.3,
            y1: 0.,
            x2: 0.7,
            y2: 1.,
        },
    };
    let request = KeyframeLegacyConversionRequest {
        project_id: "project".into(),
        expected_revision: "head".into(),
        idempotency_key: "review-lookup".into(),
        label: "Convert reviewed lookup".into(),
        sequence_id: "scene".into(),
        channel_id: "animation".into(),
        left_key_ids: vec!["left".into()],
        maximum_absolute_error: 1e-6,
    };
    let old = store.project.clone();
    let reviewed = crate::keyframe_legacy_conversion::plan(&store, &request).unwrap();
    assert_eq!(reviewed.converted_segments, 1);
    assert_eq!(reviewed.inserted_keys, 31);
    assert_eq!(reviewed.plan.idempotency_key, request.idempotency_key);
    assert_eq!(store.project, old);
    assert!(reviewed.error_bound <= request.maximum_absolute_error);
    store.project.sequences[0].tracks[0].locked = true;
    assert_eq!(
        crate::keyframe_legacy_conversion::plan(&store, &request)
            .unwrap_err()
            .code,
        "TRACK_LOCKED"
    );
    store.project.sequences[0].tracks[0].locked = false;
    store.auth.set(0);
    store.revoke = true;
    assert_eq!(
        crate::keyframe_legacy_conversion::plan(&store, &request)
            .unwrap_err()
            .code,
        "NOT_AUTHORIZED"
    );
    store.revoke = false;
    let mut stale = request;
    stale.expected_revision = "older".into();
    assert_eq!(
        crate::keyframe_legacy_conversion::plan(&store, &stale)
            .unwrap_err()
            .code,
        "REVISION_CONFLICT"
    );
}
struct Store {
    project: EditorDocument,
    auth: Cell<usize>,
    advance: bool,
    revoke: bool,
}
impl Store {
    fn new() -> Self {
        let mut project = crate::scene::new_document(&CreateProject {
            id: "project".into(),
            name: "Animation".into(),
            sequence_id: "scene".into(),
            width: 320,
            height: 180,
            fps: FrameRate {
                numerator: 30,
                denominator: 1,
            },
        })
        .unwrap();
        let mut main = project.sequences[0].tracks[0].clone();
        main.id = "main-video".into();
        project.sequences[0].main_track_id = main.id.clone();
        project.sequences[0].tracks.push(main);
        project.sequences[0].tracks[0].kind = SceneTrackKind::Graphic;
        project.sequences[0].tracks[0].elements.push(serde_json::from_value(serde_json::json!({"id":"target","name":"Rectangle","enabled":true,"timeline_range":{"start":t(3),"duration":t(10)},"source_span":{"profile":"classic_trimmed","trim_start":t(0),"trim_end":t(0),"source_duration":null},"content":{"kind":"graphic","graphic":{"source_size":{"width":100,"height":100},"fill":{"encoding":"srgb","red":1,"green":0,"blue":0,"alpha":1},"stroke":{"color":{"encoding":"srgb","red":0,"green":0,"blue":0,"alpha":1},"width_px":0,"align":"center"},"geometry":{"kind":"rectangle","corner_radius_percent":0}}},"channels":[channel()],"visual":{"profile":"classic_canvas_v31","transform":{"position_x_px":0,"position_y_px":0,"scale_x":1,"scale_y":1,"rotation_degrees":0},"opacity":1,"blend_mode":"normal","effects":[],"masks":[]}})).unwrap());
        Self {
            project,
            auth: Cell::new(0),
            advance: false,
            revoke: false,
        }
    }
    fn request(&self, edits: Vec<KeyframeEdit>) -> KeyframeEditRequest {
        KeyframeEditRequest {
            project_id: "project".into(),
            expected_revision: "head".into(),
            idempotency_key: "edit-keyframes".into(),
            label: "Edit keyframes".into(),
            sequence_id: "scene".into(),
            edits,
        }
    }
}
impl SceneRead for Store {
    fn authorize_project(&self, _: SceneCommitIntent<'_>) -> Result<(), EditorError> {
        self.auth.set(self.auth.get() + 1);
        if self.revoke && self.auth.get() > 1 {
            Err(EditorError::new("NOT_AUTHORIZED", "Revoked"))
        } else {
            Ok(())
        }
    }
    fn head_scene_source(&self, _: &str) -> Result<StoredProjectRevision, EditorError> {
        Ok(StoredProjectRevision {
            document: StoredProjectDocument::Editable(Box::new(self.project.clone())),
            revision: if self.advance && self.auth.get() > 1 {
                "advanced"
            } else {
                "head"
            }
            .into(),
        })
    }
    fn revision_scene_source(
        &self,
        project: &str,
        _: &str,
    ) -> Result<StoredProjectRevision, EditorError> {
        self.head_scene_source(project)
    }
    fn replay_scene_create(
        &self,
        _: &CreateProjectRequest,
    ) -> Result<Option<EditorSnapshot>, EditorError> {
        unreachable!()
    }
    fn replay_scene_copy(
        &self,
        _: &CopySceneProject,
    ) -> Result<Option<EditorSnapshot>, EditorError> {
        unreachable!()
    }
    fn replay_scene_edit(
        &self,
        _: SceneCommitIntent<'_>,
    ) -> Result<Option<TransactionResult>, EditorError> {
        unreachable!()
    }
}
impl ProjectPublicResources for Store {
    fn authorized_project_media_scene(
        &self,
        _: &str,
        _: &str,
        _: &AssetVersion,
    ) -> Result<ProjectAsset, EditorError> {
        unreachable!()
    }
}
fn refs() -> Vec<KeyframeRef> {
    vec![
        KeyframeRef {
            channel_id: "animation".into(),
            key_id: "left".into(),
        },
        KeyframeRef {
            channel_id: "animation".into(),
            key_id: "right".into(),
        },
    ]
}
fn pop() -> KeyframeEdit {
    KeyframeEdit::Curve {
        channel_id: "animation".into(),
        left_key_id: "left".into(),
        right_key_id: "right".into(),
        curve: CurvePreset {
            x1: 0.175,
            y1: 0.885,
            x2: 0.32,
            y2: 1.275,
        },
        reference_span_value: None,
    }
}
fn result_channel(result: &ApplyProject) -> AnimationChannel {
    let ProjectEditOperation::ElementPut { element, .. } = &result.operations[0] else {
        panic!("one element operation")
    };
    element.channels[0].clone()
}
#[test]
fn classic_pop_preserves_overshoot_and_old_normalized_contract() {
    let s = Store::new();
    let result = plan(&s, &s.request(vec![pop()])).unwrap();
    let channel = result_channel(&result);
    let AnimationChannel::Scalar { keys, .. } = &channel else {
        unreachable!()
    };
    assert_eq!(
        keys[0].right_handle.as_ref().unwrap().dt,
        Time {
            ticks: "42000".into(),
            timebase: 120000
        }
    );
    assert!((keys[1].left_handle.as_ref().unwrap().dv - 27.5).abs() < 1e-12);
    let sample = opencut_editor_core::scene::sample_animation_channel_scene(
        &channel,
        &Time {
            ticks: "3".into(),
            timebase: 2,
        },
    )
    .unwrap();
    let opencut_editor_core::scene::SampledChannel::Scalar { raw, physical, .. } = sample else {
        unreachable!()
    };
    assert!(raw > 100. && physical == raw);
    let mut old = channel.clone();
    if let AnimationChannel::Scalar { keys, .. } = &mut old {
        keys[0].segment_to_next = ScalarSegment::LegacyNormalizedBezier {
            curve: opencut_contracts::historical_visual::HistoricalVisualCubicBezier {
                x1: 0.,
                y1: 0.,
                x2: 1.,
                y2: 1.275,
            },
        };
    }
    assert!(opencut_editor_core::scene::sample_animation_channel_scene(&old, &t(1)).is_err());
}
#[test]
fn group_move_compiles_one_owner_and_keeps_curves_and_extrapolation() {
    let s = Store::new();
    let result = plan(
        &s,
        &s.request(vec![
            pop(),
            KeyframeEdit::Move {
                keys: refs(),
                delta: Time {
                    ticks: "1".into(),
                    timebase: 3,
                },
            },
        ]),
    )
    .unwrap();
    assert_eq!(result.operations.len(), 1);
    let AnimationChannel::Scalar {
        keys,
        extrapolation,
        ..
    } = result_channel(&result)
    else {
        unreachable!()
    };
    assert_eq!(
        keys[0].time,
        Time {
            ticks: "1".into(),
            timebase: 3
        }
    );
    assert_eq!(
        keys[1].time,
        Time {
            ticks: "7".into(),
            timebase: 3
        }
    );
    assert!(keys[0].right_handle.is_some());
    assert_eq!(extrapolation.after, ExtrapolationMode::Hold);
    assert_eq!(
        s.project.sequences[0].tracks[0].elements[0].channels[0],
        channel()
    );
}
#[test]
fn collision_negative_duration_and_lock_reject_complete_batch() {
    for delta in [-1, 2, 11] {
        let s = Store::new();
        let result = plan(
            &s,
            &s.request(vec![KeyframeEdit::Move {
                keys: vec![refs()[0].clone()],
                delta: t(delta),
            }]),
        );
        assert!(result.is_err(), "delta {delta}");
    }
    let mut s = Store::new();
    s.project.sequences[0].tracks[0].locked = true;
    assert_eq!(
        plan(&s, &s.request(vec![pop()])).unwrap_err().code,
        "TRACK_LOCKED"
    );
}
#[test]
fn paste_rebases_exact_time_keeps_all_handles_and_rejects_reused_ids() {
    let s = Store::new();
    let mut k = key("fresh", 1, 50.);
    k.left_handle = Some(CurveHandle {
        dt: Time {
            ticks: "-1".into(),
            timebase: 3,
        },
        dv: -80.,
    });
    k.right_handle = Some(CurveHandle {
        dt: Time {
            ticks: "1".into(),
            timebase: 3,
        },
        dv: 80.,
    });
    let edit = KeyframeEdit::PasteScalar {
        channel_id: "animation".into(),
        keys: vec![k.clone()],
        source_anchor: t(1),
        destination_anchor: t(4),
    };
    let result = plan(&s, &s.request(vec![edit])).unwrap();
    let AnimationChannel::Scalar { keys, .. } = result_channel(&result) else {
        unreachable!()
    };
    let pasted = keys.iter().find(|k| k.id == "fresh").unwrap();
    assert_eq!(pasted.time, t(4));
    assert_eq!(pasted.left_handle, k.left_handle);
    assert_eq!(pasted.right_handle, k.right_handle);
    k.id = "left".into();
    assert!(
        plan(
            &s,
            &s.request(vec![KeyframeEdit::PasteScalar {
                channel_id: "animation".into(),
                keys: vec![k],
                source_anchor: t(1),
                destination_anchor: t(4)
            }])
        )
        .is_err()
    );
}
#[test]
fn deleting_last_keys_removes_channel_in_one_undoable_owner() {
    let s = Store::new();
    let result = plan(&s, &s.request(vec![KeyframeEdit::Delete { keys: refs() }])).unwrap();
    let ProjectEditOperation::ElementPut { element, .. } = &result.operations[0] else {
        unreachable!()
    };
    assert!(element.channels.is_empty());
    assert_eq!(result.operations.len(), 1);
}
#[test]
fn stale_revision_and_withdrawn_authorization_never_return_transaction() {
    let mut s = Store::new();
    s.advance = true;
    assert!(plan(&s, &s.request(vec![pop()])).is_err());
    let mut s = Store::new();
    s.revoke = true;
    assert_eq!(
        plan(&s, &s.request(vec![pop()])).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
}
#[test]
fn flat_segment_requires_reference_and_allows_real_value_bump() {
    let mut s = Store::new();
    if let AnimationChannel::Scalar { keys, .. } =
        &mut s.project.sequences[0].tracks[0].elements[0].channels[0]
    {
        keys[1].value = 0.;
    }
    assert!(plan(&s, &s.request(vec![pop()])).is_err());
    let mut edit = pop();
    if let KeyframeEdit::Curve {
        reference_span_value,
        ..
    } = &mut edit
    {
        *reference_span_value = Some(100.);
    }
    let c = result_channel(&plan(&s, &s.request(vec![edit])).unwrap());
    let sample = opencut_editor_core::scene::sample_animation_channel_scene(&c, &t(1)).unwrap();
    let opencut_editor_core::scene::SampledChannel::Scalar { raw, .. } = sample else {
        unreachable!()
    };
    assert!(raw > 0.);
}
#[test]
fn inapplicable_destination_parameter_rejects_instead_of_losing_key() {
    let s = Store::new();
    let mut c = channel();
    if let AnimationChannel::Scalar { binding, .. } = &mut c {
        *binding = ScalarBinding::Property {
            parameter: ScalarParameter::FontSizeClassicHeight90,
        };
    }
    assert!(
        plan(
            &s,
            &s.request(vec![KeyframeEdit::ChannelPut {
                owner: KeyframeOwner::Element {
                    element_id: "target".into()
                },
                channel: c
            }])
        )
        .is_err()
    );
}
#[test]
fn independent_polynomial_oracle_distinguishes_raw_overshoot_and_physical_gain_limit() {
    let mut s = Store::new();
    let c = CurvePreset {
        x1: 1.0 / 3.0,
        y1: 1.5,
        x2: 2.0 / 3.0,
        y2: 1.5,
    };
    let edit = KeyframeEdit::Curve {
        channel_id: "animation".into(),
        left_key_id: "left".into(),
        right_key_id: "right".into(),
        curve: c,
        reference_span_value: None,
    };
    let result = plan(&s, &s.request(vec![edit])).unwrap();
    let channel = result_channel(&result);
    // At t=.5, x=.5 and y=3/8*1.5+3/8*1.5+1/8=1.25.
    let opencut_editor_core::scene::SampledChannel::Scalar { raw, physical, .. } =
        opencut_editor_core::scene::sample_animation_channel_scene(&channel, &t(1)).unwrap()
    else {
        unreachable!()
    };
    assert!((raw - 125.).abs() < 1e-10);
    assert_eq!(raw, physical);
    let mut gain = channel.clone();
    if let AnimationChannel::Scalar { binding, keys, .. } = &mut gain {
        *binding = ScalarBinding::Property {
            parameter: ScalarParameter::GainDecibels,
        };
        keys[0].value = -60.;
        keys[1].value = 20.;
        keys[0].right_handle.as_mut().unwrap().dv = 120.;
        keys[1].left_handle.as_mut().unwrap().dv = 40.;
    }
    let opencut_editor_core::scene::SampledChannel::Scalar { raw, physical, .. } =
        opencut_editor_core::scene::sample_animation_channel_scene(&gain, &t(1)).unwrap()
    else {
        unreachable!()
    };
    assert!((raw - 40.).abs() < 1e-10);
    assert_eq!(physical, 20.);
    // Changing stored values to a physical parameter with incompatible endpoint
    // range is rejected by final aggregate validation, never clamped on write.
    if let AnimationChannel::Scalar { binding, .. } =
        &mut s.project.sequences[0].tracks[0].elements[0].channels[0]
    {
        *binding = ScalarBinding::Property {
            parameter: ScalarParameter::Opacity,
        };
    }
    assert!(plan(&s, &s.request(vec![pop()])).is_err());
}
#[test]
fn discrete_and_track_owned_edits_preserve_type_clock_and_one_atomic_owner() {
    let mut s = Store::new();
    let track_id = s.project.sequences[0].tracks[0].id.clone();
    s.project.sequences[0].tracks[0]
        .channels
        .push(AnimationChannel::Scalar {
            id: "track-opacity".into(),
            target: ChannelTarget::Track { track_id },
            binding: ScalarBinding::Property {
                parameter: ScalarParameter::Opacity,
            },
            keys: vec![key("track-key", 5, 0.5)],
            extrapolation: Extrapolation {
                before: ExtrapolationMode::Hold,
                after: ExtrapolationMode::Hold,
            },
        });
    let result = plan(
        &s,
        &s.request(vec![
            KeyframeEdit::Move {
                keys: vec![KeyframeRef {
                    channel_id: "track-opacity".into(),
                    key_id: "track-key".into(),
                }],
                delta: t(1),
            },
            pop(),
        ]),
    )
    .unwrap();
    assert_eq!(result.operations.len(), 1);
    let ProjectEditOperation::TrackPut { track, .. } = &result.operations[0] else {
        unreachable!()
    };
    assert_eq!(track.elements[0].channels.len(), 1);
    let AnimationChannel::Scalar { keys, .. } = &track.channels[0] else {
        unreachable!()
    };
    assert_eq!(keys[0].time, t(6));
    let target = ChannelTarget::Element {
        element_id: "target".into(),
    };
    let c = AnimationChannel::Discrete {
        id: "stroke-align".into(),
        target,
        parameter: DiscreteParameter::GraphicStrokeAlign,
        keys: vec![DiscreteKey {
            id: "align-left".into(),
            time: t(0),
            value: DiscreteValue::StrokeAlign {
                value: StrokeAlign::Inside,
            },
        }],
    };
    let result = plan(
        &s,
        &s.request(vec![
            KeyframeEdit::ChannelPut {
                owner: KeyframeOwner::Element {
                    element_id: "target".into(),
                },
                channel: c,
            },
            KeyframeEdit::PasteDiscrete {
                channel_id: "stroke-align".into(),
                keys: vec![DiscreteKey {
                    id: "align-new".into(),
                    time: t(2),
                    value: DiscreteValue::StrokeAlign {
                        value: StrokeAlign::Outside,
                    },
                }],
                source_anchor: t(2),
                destination_anchor: t(5),
            },
        ]),
    )
    .unwrap();
    let ProjectEditOperation::ElementPut { element, .. } = &result.operations[0] else {
        unreachable!()
    };
    let AnimationChannel::Discrete { keys, .. } = &element.channels[1] else {
        unreachable!()
    };
    assert_eq!(keys[1].time, t(5));
    assert_eq!(
        keys[1].value,
        DiscreteValue::StrokeAlign {
            value: StrokeAlign::Outside
        }
    );
}
#[test]
fn duplicate_selection_and_tangent_overrun_fail_without_partial_proposal() {
    let s = Store::new();
    let r = refs()[0].clone();
    assert!(
        plan(
            &s,
            &s.request(vec![KeyframeEdit::Move {
                keys: vec![r.clone(), r],
                delta: t(1)
            }])
        )
        .is_err()
    );
    let result = plan(
        &s,
        &s.request(vec![
            pop(),
            KeyframeEdit::Move {
                keys: vec![refs()[1].clone()],
                delta: Time {
                    ticks: "-19".into(),
                    timebase: 10,
                },
            },
        ]),
    );
    assert!(result.is_err());
    assert_eq!(
        s.project.sequences[0].tracks[0].elements[0].channels[0],
        channel()
    );
}

#[test]
fn fixed_curve_read_preserves_requested_local_time_order_and_uses_shared_sampler() {
    use opencut_contracts::keyframe_sample::*;
    let store = Store::new();
    let mut input = KeyframeSampleRequest {
        project_id: "project".into(),
        revision: "head".into(),
        sequence_id: "scene".into(),
        channel_id: "animation".into(),
        times: vec![t(2), t(0), t(1)],
    };
    let result = crate::keyframe_sample::sample(&store, &input).unwrap();
    assert_eq!(
        result.samples.iter().map(|s| s.value).collect::<Vec<_>>(),
        vec![100., 0., 50.]
    );
    assert_eq!(
        result
            .samples
            .iter()
            .map(|s| s.time.clone())
            .collect::<Vec<_>>(),
        input.times
    );
    input.times = vec![t(0); 1025];
    assert_eq!(
        crate::keyframe_sample::sample(&store, &input)
            .unwrap_err()
            .code,
        "INVALID_ARGUMENT"
    );
    input.times = vec![t(0)];
    input.revision = "other".into();
    assert_eq!(
        crate::keyframe_sample::sample(&store, &input)
            .unwrap_err()
            .code,
        "STORAGE_ERROR"
    );
}

#[test]
fn insertion_splits_time_value_controls_without_changing_the_curve_or_other_ids() {
    let mut original = channel();
    let AnimationChannel::Scalar { keys, .. } = &mut original else {
        panic!()
    };
    keys[0].segment_to_next = ScalarSegment::Bezier;
    keys[0].right_handle = Some(CurveHandle {
        dt: Time {
            ticks: "4".into(),
            timebase: 10,
        },
        dv: 30.,
    });
    keys[1].left_handle = Some(CurveHandle {
        dt: Time {
            ticks: "-4".into(),
            timebase: 10,
        },
        dv: -10.,
    });
    let mut seq = Store::new().project.sequences.remove(0);
    seq.tracks[0].elements[0].channels = vec![original.clone()];
    edits::apply(
        &mut seq,
        &KeyframeEdit::InsertScalar {
            channel_id: "animation".into(),
            key_id: "new-key".into(),
            time: t(1),
        },
    )
    .unwrap();
    let edited = &seq.tracks[0].elements[0].channels[0];
    let AnimationChannel::Scalar { keys, .. } = edited else {
        panic!()
    };
    assert_eq!(
        keys.iter().map(|k| k.id.as_str()).collect::<Vec<_>>(),
        ["left", "new-key", "right"]
    );
    for i in 0..=200 {
        let at = Time {
            ticks: i.to_string(),
            timebase: 100,
        };
        let raw =
            |c| match opencut_editor_core::scene::sample_animation_channel_scene(c, &at).unwrap() {
                opencut_editor_core::scene::SampledChannel::Scalar { raw, .. } => raw,
                _ => panic!(),
            };
        assert!((raw(&original) - raw(edited)).abs() < 0.001, "sample {i}");
    }
    assert!(
        edits::apply(
            &mut seq,
            &KeyframeEdit::InsertScalar {
                channel_id: "animation".into(),
                key_id: "another".into(),
                time: t(1)
            }
        )
        .is_err()
    );
}
