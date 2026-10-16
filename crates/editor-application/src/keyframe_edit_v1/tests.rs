use super::*;
use crate::project_v4::{ProjectV4Read, StoredProjectRevisionV4};
use opencut_contracts::{
    AssetVersion, CreateProject, FrameRate, TransactionResult,
    project_v4_ops::{CopyV4Project, CreateV4Project},
};
use std::cell::Cell;
fn t(n: i64) -> Time {
    Time {
        ticks: n.to_string(),
        timebase: 1,
    }
}
fn key(id: &str, time: i64, value: f64) -> ScalarKeyV4 {
    ScalarKeyV4 {
        id: id.into(),
        segment_id: None,
        time: t(time),
        value,
        left_handle: None,
        right_handle: None,
        segment_to_next: ScalarSegmentV4::Linear,
        tangent_mode: TangentModeV4::Broken,
    }
}
fn channel() -> AnimationChannelV4 {
    AnimationChannelV4::Scalar {
        id: "animation".into(),
        target: ChannelTargetV4::Element {
            element_id: "target".into(),
        },
        binding: ScalarBindingV4::Property {
            parameter: ScalarParameterV4::PositionXPx,
        },
        keys: vec![key("left", 0, 0.), key("right", 2, 100.)],
        extrapolation: ExtrapolationV4 {
            before: ExtrapolationModeV4::Hold,
            after: ExtrapolationModeV4::Hold,
        },
    }
}

#[test]
fn legacy_conversion_is_a_read_only_fixed_head_public_plan_and_rechecks_authority() {
    use opencut_contracts::keyframe_legacy_conversion_v1::*;
    let mut store = Store::new();
    let AnimationChannelV4::Scalar { keys, .. } =
        &mut store.project.sequences[0].tracks[0].elements[0].channels[0]
    else {
        panic!()
    };
    keys[0].segment_to_next = ScalarSegmentV4::LegacyNormalizedBezier {
        curve: opencut_contracts::project_v3::CubicBezierV1 {
            x1: 0.3,
            y1: 0.,
            x2: 0.7,
            y2: 1.,
        },
    };
    let request = KeyframeLegacyConversionRequestV1 {
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
    let reviewed = crate::keyframe_legacy_conversion_v1::plan(&store, &request).unwrap();
    assert_eq!(reviewed.converted_segments, 1);
    assert_eq!(reviewed.inserted_keys, 31);
    assert_eq!(reviewed.plan.idempotency_key, request.idempotency_key);
    assert_eq!(store.project, old);
    assert!(reviewed.error_bound <= request.maximum_absolute_error);
    store.project.sequences[0].tracks[0].locked = true;
    assert_eq!(
        crate::keyframe_legacy_conversion_v1::plan(&store, &request)
            .unwrap_err()
            .code,
        "TRACK_LOCKED"
    );
    store.project.sequences[0].tracks[0].locked = false;
    store.auth.set(0);
    store.revoke = true;
    assert_eq!(
        crate::keyframe_legacy_conversion_v1::plan(&store, &request)
            .unwrap_err()
            .code,
        "NOT_AUTHORIZED"
    );
    store.revoke = false;
    let mut stale = request;
    stale.expected_revision = "older".into();
    assert_eq!(
        crate::keyframe_legacy_conversion_v1::plan(&store, &stale)
            .unwrap_err()
            .code,
        "REVISION_CONFLICT"
    );
}
struct Store {
    project: ProjectDocumentV4,
    auth: Cell<usize>,
    advance: bool,
    revoke: bool,
}
impl Store {
    fn new() -> Self {
        let mut project = crate::project_v4::new_document(&CreateProject {
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
        project.sequences[0].tracks[0].kind = TrackKindV4::Graphic;
        project.sequences[0].tracks[0].elements.push(serde_json::from_value(serde_json::json!({"id":"target","name":"Rectangle","enabled":true,"timeline_range":{"start":t(3),"duration":t(10)},"source_span":{"profile":"classic_trimmed","trim_start":t(0),"trim_end":t(0),"source_duration":null},"content":{"kind":"graphic","graphic":{"source_size":{"width":100,"height":100},"fill":{"encoding":"srgb","red":1,"green":0,"blue":0,"alpha":1},"stroke":{"color":{"encoding":"srgb","red":0,"green":0,"blue":0,"alpha":1},"width_px":0,"align":"center"},"geometry":{"kind":"rectangle","corner_radius_percent":0}}},"channels":[channel()],"visual":{"profile":"classic_canvas_v31","transform":{"position_x_px":0,"position_y_px":0,"scale_x":1,"scale_y":1,"rotation_degrees":0},"opacity":1,"blend_mode":"normal","effects":[],"masks":[]}})).unwrap());
        Self {
            project,
            auth: Cell::new(0),
            advance: false,
            revoke: false,
        }
    }
    fn request(&self, edits: Vec<KeyframeEditV1>) -> KeyframeEditRequestV1 {
        KeyframeEditRequestV1 {
            project_id: "project".into(),
            expected_revision: "head".into(),
            idempotency_key: "edit-keyframes".into(),
            label: "Edit keyframes".into(),
            sequence_id: "scene".into(),
            edits,
        }
    }
}
impl ProjectV4Read for Store {
    fn authorize_v4(&self, _: V4Intent<'_>) -> Result<(), EditorError> {
        self.auth.set(self.auth.get() + 1);
        if self.revoke && self.auth.get() > 1 {
            Err(EditorError::new("NOT_AUTHORIZED", "Revoked"))
        } else {
            Ok(())
        }
    }
    fn head_v4_source(&self, _: &str) -> Result<StoredProjectRevisionV4, EditorError> {
        Ok(StoredProjectRevisionV4 {
            document: StoredProjectDocumentV4::V4(Box::new(self.project.clone())),
            revision: if self.advance && self.auth.get() > 1 {
                "advanced"
            } else {
                "head"
            }
            .into(),
        })
    }
    fn revision_v4_source(
        &self,
        project: &str,
        _: &str,
    ) -> Result<StoredProjectRevisionV4, EditorError> {
        self.head_v4_source(project)
    }
    fn replay_v4_create(
        &self,
        _: &CreateV4Project,
    ) -> Result<Option<ProjectSnapshotV4>, EditorError> {
        unreachable!()
    }
    fn replay_v4_copy(&self, _: &CopyV4Project) -> Result<Option<ProjectSnapshotV4>, EditorError> {
        unreachable!()
    }
    fn replay_v4_edit(&self, _: V4Intent<'_>) -> Result<Option<TransactionResult>, EditorError> {
        unreachable!()
    }
}
impl ProjectV4PublicResources for Store {
    fn authorized_project_media_v4(
        &self,
        _: &str,
        _: &str,
        _: &AssetVersion,
    ) -> Result<AssetV4, EditorError> {
        unreachable!()
    }
}
fn refs() -> Vec<KeyframeRefV1> {
    vec![
        KeyframeRefV1 {
            channel_id: "animation".into(),
            key_id: "left".into(),
        },
        KeyframeRefV1 {
            channel_id: "animation".into(),
            key_id: "right".into(),
        },
    ]
}
fn pop() -> KeyframeEditV1 {
    KeyframeEditV1::Curve {
        channel_id: "animation".into(),
        left_key_id: "left".into(),
        right_key_id: "right".into(),
        curve: CurvePresetV1 {
            x1: 0.175,
            y1: 0.885,
            x2: 0.32,
            y2: 1.275,
        },
        reference_span_value: None,
    }
}
fn result_channel(result: &ApplyPublicV4Project) -> AnimationChannelV4 {
    let PublicProjectOperationV4::ElementPut { element, .. } = &result.operations[0] else {
        panic!("one element operation")
    };
    element.channels[0].clone()
}
#[test]
fn classic_pop_preserves_overshoot_and_old_normalized_contract() {
    let s = Store::new();
    let result = plan(&s, &s.request(vec![pop()])).unwrap();
    let channel = result_channel(&result);
    let AnimationChannelV4::Scalar { keys, .. } = &channel else {
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
    let sample = opencut_editor_core::project_v4::sample_animation_channel_v4(
        &channel,
        &Time {
            ticks: "3".into(),
            timebase: 2,
        },
    )
    .unwrap();
    let opencut_editor_core::project_v4::SampledChannelV4::Scalar { raw, physical, .. } = sample
    else {
        unreachable!()
    };
    assert!(raw > 100. && physical == raw);
    let mut old = channel.clone();
    if let AnimationChannelV4::Scalar { keys, .. } = &mut old {
        keys[0].segment_to_next = ScalarSegmentV4::LegacyNormalizedBezier {
            curve: opencut_contracts::project_v3::CubicBezierV1 {
                x1: 0.,
                y1: 0.,
                x2: 1.,
                y2: 1.275,
            },
        };
    }
    assert!(opencut_editor_core::project_v4::sample_animation_channel_v4(&old, &t(1)).is_err());
}
#[test]
fn group_move_compiles_one_owner_and_keeps_curves_and_extrapolation() {
    let s = Store::new();
    let result = plan(
        &s,
        &s.request(vec![
            pop(),
            KeyframeEditV1::Move {
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
    let AnimationChannelV4::Scalar {
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
    assert_eq!(extrapolation.after, ExtrapolationModeV4::Hold);
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
            &s.request(vec![KeyframeEditV1::Move {
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
    k.left_handle = Some(CurveHandleV4 {
        dt: Time {
            ticks: "-1".into(),
            timebase: 3,
        },
        dv: -80.,
    });
    k.right_handle = Some(CurveHandleV4 {
        dt: Time {
            ticks: "1".into(),
            timebase: 3,
        },
        dv: 80.,
    });
    let edit = KeyframeEditV1::PasteScalar {
        channel_id: "animation".into(),
        keys: vec![k.clone()],
        source_anchor: t(1),
        destination_anchor: t(4),
    };
    let result = plan(&s, &s.request(vec![edit])).unwrap();
    let AnimationChannelV4::Scalar { keys, .. } = result_channel(&result) else {
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
            &s.request(vec![KeyframeEditV1::PasteScalar {
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
    let result = plan(
        &s,
        &s.request(vec![KeyframeEditV1::Delete { keys: refs() }]),
    )
    .unwrap();
    let PublicProjectOperationV4::ElementPut { element, .. } = &result.operations[0] else {
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
    if let AnimationChannelV4::Scalar { keys, .. } =
        &mut s.project.sequences[0].tracks[0].elements[0].channels[0]
    {
        keys[1].value = 0.;
    }
    assert!(plan(&s, &s.request(vec![pop()])).is_err());
    let mut edit = pop();
    if let KeyframeEditV1::Curve {
        reference_span_value,
        ..
    } = &mut edit
    {
        *reference_span_value = Some(100.);
    }
    let c = result_channel(&plan(&s, &s.request(vec![edit])).unwrap());
    let sample = opencut_editor_core::project_v4::sample_animation_channel_v4(&c, &t(1)).unwrap();
    let opencut_editor_core::project_v4::SampledChannelV4::Scalar { raw, .. } = sample else {
        unreachable!()
    };
    assert!(raw > 0.);
}
#[test]
fn inapplicable_destination_parameter_rejects_instead_of_losing_key() {
    let s = Store::new();
    let mut c = channel();
    if let AnimationChannelV4::Scalar { binding, .. } = &mut c {
        *binding = ScalarBindingV4::Property {
            parameter: ScalarParameterV4::FontSizeClassicHeight90,
        };
    }
    assert!(
        plan(
            &s,
            &s.request(vec![KeyframeEditV1::ChannelPut {
                owner: KeyframeOwnerV1::Element {
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
    let c = CurvePresetV1 {
        x1: 1.0 / 3.0,
        y1: 1.5,
        x2: 2.0 / 3.0,
        y2: 1.5,
    };
    let edit = KeyframeEditV1::Curve {
        channel_id: "animation".into(),
        left_key_id: "left".into(),
        right_key_id: "right".into(),
        curve: c,
        reference_span_value: None,
    };
    let result = plan(&s, &s.request(vec![edit])).unwrap();
    let channel = result_channel(&result);
    // At t=.5, x=.5 and y=3/8*1.5+3/8*1.5+1/8=1.25.
    let opencut_editor_core::project_v4::SampledChannelV4::Scalar { raw, physical, .. } =
        opencut_editor_core::project_v4::sample_animation_channel_v4(&channel, &t(1)).unwrap()
    else {
        unreachable!()
    };
    assert!((raw - 125.).abs() < 1e-10);
    assert_eq!(raw, physical);
    let mut gain = channel.clone();
    if let AnimationChannelV4::Scalar { binding, keys, .. } = &mut gain {
        *binding = ScalarBindingV4::Property {
            parameter: ScalarParameterV4::GainDecibels,
        };
        keys[0].value = -60.;
        keys[1].value = 20.;
        keys[0].right_handle.as_mut().unwrap().dv = 120.;
        keys[1].left_handle.as_mut().unwrap().dv = 40.;
    }
    let opencut_editor_core::project_v4::SampledChannelV4::Scalar { raw, physical, .. } =
        opencut_editor_core::project_v4::sample_animation_channel_v4(&gain, &t(1)).unwrap()
    else {
        unreachable!()
    };
    assert!((raw - 40.).abs() < 1e-10);
    assert_eq!(physical, 20.);
    // Changing stored values to a physical parameter with incompatible endpoint
    // range is rejected by final aggregate validation, never clamped on write.
    if let AnimationChannelV4::Scalar { binding, .. } =
        &mut s.project.sequences[0].tracks[0].elements[0].channels[0]
    {
        *binding = ScalarBindingV4::Property {
            parameter: ScalarParameterV4::Opacity,
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
        .push(AnimationChannelV4::Scalar {
            id: "track-opacity".into(),
            target: ChannelTargetV4::Track { track_id },
            binding: ScalarBindingV4::Property {
                parameter: ScalarParameterV4::Opacity,
            },
            keys: vec![key("track-key", 5, 0.5)],
            extrapolation: ExtrapolationV4 {
                before: ExtrapolationModeV4::Hold,
                after: ExtrapolationModeV4::Hold,
            },
        });
    let result = plan(
        &s,
        &s.request(vec![
            KeyframeEditV1::Move {
                keys: vec![KeyframeRefV1 {
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
    let PublicProjectOperationV4::TrackPut { track, .. } = &result.operations[0] else {
        unreachable!()
    };
    assert_eq!(track.elements[0].channels.len(), 1);
    let AnimationChannelV4::Scalar { keys, .. } = &track.channels[0] else {
        unreachable!()
    };
    assert_eq!(keys[0].time, t(6));
    let target = ChannelTargetV4::Element {
        element_id: "target".into(),
    };
    let c = AnimationChannelV4::Discrete {
        id: "stroke-align".into(),
        target,
        parameter: DiscreteParameterV4::GraphicStrokeAlign,
        keys: vec![DiscreteKeyV4 {
            id: "align-left".into(),
            time: t(0),
            value: DiscreteValueV4::StrokeAlign {
                value: StrokeAlignV4::Inside,
            },
        }],
    };
    let result = plan(
        &s,
        &s.request(vec![
            KeyframeEditV1::ChannelPut {
                owner: KeyframeOwnerV1::Element {
                    element_id: "target".into(),
                },
                channel: c,
            },
            KeyframeEditV1::PasteDiscrete {
                channel_id: "stroke-align".into(),
                keys: vec![DiscreteKeyV4 {
                    id: "align-new".into(),
                    time: t(2),
                    value: DiscreteValueV4::StrokeAlign {
                        value: StrokeAlignV4::Outside,
                    },
                }],
                source_anchor: t(2),
                destination_anchor: t(5),
            },
        ]),
    )
    .unwrap();
    let PublicProjectOperationV4::ElementPut { element, .. } = &result.operations[0] else {
        unreachable!()
    };
    let AnimationChannelV4::Discrete { keys, .. } = &element.channels[1] else {
        unreachable!()
    };
    assert_eq!(keys[1].time, t(5));
    assert_eq!(
        keys[1].value,
        DiscreteValueV4::StrokeAlign {
            value: StrokeAlignV4::Outside
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
            &s.request(vec![KeyframeEditV1::Move {
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
            KeyframeEditV1::Move {
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
    use opencut_contracts::keyframe_sample_v1::*;
    let store = Store::new();
    let mut input = KeyframeSampleRequestV1 {
        project_id: "project".into(),
        revision: "head".into(),
        sequence_id: "scene".into(),
        channel_id: "animation".into(),
        times: vec![t(2), t(0), t(1)],
    };
    let result = crate::keyframe_sample_v1::sample(&store, &input).unwrap();
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
        crate::keyframe_sample_v1::sample(&store, &input)
            .unwrap_err()
            .code,
        "INVALID_ARGUMENT"
    );
    input.times = vec![t(0)];
    input.revision = "other".into();
    assert_eq!(
        crate::keyframe_sample_v1::sample(&store, &input)
            .unwrap_err()
            .code,
        "STORAGE_ERROR"
    );
}
