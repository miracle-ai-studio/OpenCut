use super::*;
use crate::scene::{SceneRead, StoredProjectRevision};
use opencut_contracts::{
    AssetVersion, CreateProject, FrameRate, Time, TransactionResult,
    scene_ops::{CopySceneProject, CreateProjectRequest},
};
use std::cell::Cell;
fn t(n: i64) -> opencut_contracts::Time {
    opencut_contracts::Time {
        ticks: n.to_string(),
        timebase: 1,
    }
}
struct Store {
    project: EditorDocument,
    calls: Cell<usize>,
    deny: bool,
}
impl Store {
    fn new() -> Self {
        let mut project = crate::scene::new_document(&CreateProject {
            id: "p".into(),
            name: "Properties".into(),
            sequence_id: "s".into(),
            width: 320,
            height: 180,
            fps: FrameRate {
                numerator: 30,
                denominator: 1,
            },
        })
        .unwrap();
        project.assets.push(serde_json::from_value(serde_json::json!({"id":"a","name":"Audio","kind":"audio","version":{"id":format!("av_{}","a".repeat(64)),"content_hash":"a".repeat(64),"size":1},"duration":t(10),"origin":{"kind":"legacy_locator","uri":"opencut-media://offline"},"metadata":{"mime_type":null,"intrinsic_size":null,"frame_rate":null,"rotation_degrees":null,"audio":{"status":"present","metadata":null}},"license":null})).unwrap());
        let mut track = project.sequences[0].tracks[0].clone();
        track.id = "sound".into();
        track.kind = SceneTrackKind::Audio;
        track.elements.push(serde_json::from_value(serde_json::json!({"id":"audio","name":"Source","enabled":true,"timeline_range":{"start":t(0),"duration":t(2)},"source_span":{"profile":"classic_trimmed","trim_start":t(1),"trim_end":t(6),"source_duration":t(10)},"content":{"kind":"audio","asset_id":"a","audio":{"stream":null,"gain":{"unit":"decibels","value":-6.05},"muted":false},"retime":{"rate":{"numerator":3,"denominator":2},"maintain_pitch":true,"mapping":{"kind":"classic_rounded_ticks","timebase":120000}}},"visual":null,"channels":[]})).unwrap());
        project.sequences[0].tracks.push(track);
        Self {
            project,
            calls: Cell::new(0),
            deny: false,
        }
    }
    fn request(&self) -> ElementPropertiesRequest {
        ElementPropertiesRequest {
            project_id: "p".into(),
            expected_revision: "head".into(),
            idempotency_key: "properties".into(),
            label: "Edit properties".into(),
            sequence_id: "s".into(),
            element_id: "audio".into(),
            transform: None,
            opacity: None,
            audio_gain: None,
            muted: None,
            retime: None,
        }
    }
    fn element(&self) -> &TimelineElement {
        &self.project.sequences[0].tracks[1].elements[0]
    }
}
impl SceneRead for Store {
    fn authorize_project(&self, _: SceneCommitIntent<'_>) -> Result<(), EditorError> {
        self.calls.set(self.calls.get() + 1);
        if self.deny && self.calls.get() > 1 {
            Err(EditorError::new("NOT_AUTHORIZED", "Revoked"))
        } else {
            Ok(())
        }
    }
    fn head_scene_source(&self, _: &str) -> Result<StoredProjectRevision, EditorError> {
        Ok(StoredProjectRevision {
            document: StoredProjectDocument::Editable(Box::new(self.project.clone())),
            revision: "head".into(),
        })
    }
    fn revision_scene_source(
        &self,
        _: &str,
        _: &str,
    ) -> Result<StoredProjectRevision, EditorError> {
        unreachable!()
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
fn changed(tx: &ApplyProject) -> &TimelineElement {
    let ProjectEditOperation::ElementPut { element, .. } = &tx.operations[0] else {
        unreachable!()
    };
    element
}
#[test]
fn speed_preserves_declared_visible_source_and_uses_inverse_rounding_once() {
    let s = Store::new();
    let mut input = s.request();
    input.retime = Some(Retime {
        rate: RationalRate {
            numerator: 3,
            denominator: 1,
        },
        maintain_pitch: false,
        mapping: RetimeMapping::ClassicRoundedTicks { timebase: 120000 },
    });
    let tx = plan(&s, &input).unwrap();
    let e = changed(&tx);
    assert_eq!(
        R::try_from(&e.timeline_range.duration)
            .unwrap()
            .cmp_exact(R::new(1, 1).unwrap()),
        std::cmp::Ordering::Equal
    );
    assert_eq!(e.source_span, s.element().source_span);
    assert_eq!(e.channels, s.element().channels);
    let ElementContent::Audio { retime, audio, .. } = &e.content else {
        unreachable!()
    };
    assert!(!retime.maintain_pitch);
    assert_eq!(audio.gain, Gain::Decibels { value: -6.05 });
    assert_eq!(tx.idempotency_key, input.idempotency_key);
    assert_eq!(s.element().timeline_range.duration, t(2));
}
#[test]
fn pitch_and_gain_only_keep_exact_duration_source_and_untouched_fields() {
    let s = Store::new();
    let mut input = s.request();
    let ElementContent::Audio { retime, .. } = &s.element().content else {
        unreachable!()
    };
    input.retime = Some(Retime {
        maintain_pitch: false,
        ..retime.clone()
    });
    input.audio_gain = Some(Gain::Decibels { value: -3.125 });
    input.muted = Some(true);
    let tx = plan(&s, &input).unwrap();
    let e = changed(&tx);
    assert_eq!(e.timeline_range, s.element().timeline_range);
    assert_eq!(e.source_span, s.element().source_span);
    assert_eq!(e.enabled, s.element().enabled);
}
#[test]
fn invalid_mapping_range_lock_and_revocation_never_return_transaction() {
    let mut s = Store::new();
    let mut input = s.request();
    input.retime = Some(Retime {
        rate: RationalRate {
            numerator: 3,
            denominator: 1,
        },
        maintain_pitch: true,
        mapping: RetimeMapping::ExactRational,
    });
    assert!(plan(&s, &input).is_err());
    input.retime = Some(Retime {
        rate: RationalRate {
            numerator: 6,
            denominator: 1,
        },
        maintain_pitch: true,
        mapping: RetimeMapping::ClassicRoundedTicks { timebase: 120000 },
    });
    assert!(plan(&s, &input).is_err());
    input.retime = None;
    input.muted = Some(true);
    s.project.sequences[0].tracks[1].locked = true;
    assert_eq!(plan(&s, &input).unwrap_err().code, "TRACK_LOCKED");
    s.project.sequences[0].tracks[1].locked = false;
    s.deny = true;
    assert_eq!(plan(&s, &input).unwrap_err().code, "NOT_AUTHORIZED");
}
#[test]
fn shortening_retime_clamps_like_classic_and_removes_scalar_channel_before_its_first_key() {
    let mut s = Store::new();
    s.project.sequences[0].tracks[1].elements[0]
        .channels
        .push(AnimationChannel::Scalar {
            id: "gain".into(),
            target: ChannelTarget::Element {
                element_id: "audio".into(),
            },
            binding: ScalarBinding::Property {
                parameter: ScalarParameter::GainDecibels,
            },
            keys: vec![ScalarKey {
                id: "key".into(),
                segment_id: None,
                time: t(2),
                value: -10.,
                left_handle: None,
                right_handle: None,
                segment_to_next: ScalarSegment::Linear,
                tangent_mode: TangentMode::Broken,
            }],
            extrapolation: Extrapolation {
                before: ExtrapolationMode::Hold,
                after: ExtrapolationMode::Hold,
            },
        });
    let mut input = s.request();
    input.retime = Some(Retime {
        rate: RationalRate {
            numerator: 3,
            denominator: 1,
        },
        maintain_pitch: true,
        mapping: RetimeMapping::ClassicRoundedTicks { timebase: 120000 },
    });
    let result = plan(&s, &input).unwrap();
    let ProjectEditOperation::ElementPut { element, .. } = &result.operations[0] else {
        panic!()
    };
    assert!(element.channels.is_empty());
    assert_eq!(s.element().channels.len(), 1);
}

fn animated_gain() -> AnimationChannel {
    let key = |id: &str, time: i64, value: f64| ScalarKey {
        id: id.into(),
        segment_id: None,
        time: t(time),
        value,
        left_handle: None,
        right_handle: None,
        segment_to_next: ScalarSegment::Linear,
        tangent_mode: TangentMode::Flat,
    };
    AnimationChannel::Scalar {
        id: "gain".into(),
        target: ChannelTarget::Element {
            element_id: "audio".into(),
        },
        binding: ScalarBinding::Property {
            parameter: ScalarParameter::GainDecibels,
        },
        keys: vec![key("gain-left", 0, -20.), key("gain-right", 2, 0.)],
        extrapolation: Extrapolation {
            before: ExtrapolationMode::Hold,
            after: ExtrapolationMode::Hold,
        },
    }
}
fn accelerated(s: &Store) -> ElementPropertiesRequest {
    let mut input = s.request();
    input.retime = Some(Retime {
        rate: RationalRate {
            numerator: 3,
            denominator: 1,
        },
        maintain_pitch: true,
        mapping: RetimeMapping::ClassicRoundedTicks { timebase: 120000 },
    });
    input
}
#[test]
fn speed_clamp_adds_sampled_boundary_preserves_clock_and_ids_and_is_one_atomic_operation() {
    let mut s = Store::new();
    let source = animated_gain();
    s.project.sequences[0].tracks[1].elements[0].channels = vec![source.clone()];
    let input = accelerated(&s);
    let result = plan(&s, &input).unwrap();
    assert_eq!(result.operations.len(), 1);
    let element = changed(&result);
    assert!(
        R::try_from(&element.timeline_range.duration)
            .unwrap()
            .cmp_exact(R::new(1, 1).unwrap())
            .is_eq()
    );
    let AnimationChannel::Scalar { keys, .. } = &element.channels[0] else {
        panic!()
    };
    assert_eq!(keys.len(), 2);
    assert_eq!(keys[0].id, "gain-left");
    assert_eq!(keys[0].time, t(0));
    assert!(
        R::try_from(&keys[1].time)
            .unwrap()
            .cmp_exact(R::new(1, 1).unwrap())
            .is_eq()
    );
    assert_eq!(keys[1].value, -10.);
    assert_eq!(keys[1].tangent_mode, TangentMode::Flat);
    assert_ne!(keys[1].id, "gain-right");
    assert_eq!(s.element().channels[0], source);
    assert_eq!(plan(&s, &input).unwrap(), result);
}
#[test]
fn speed_clamp_uses_existing_de_casteljau_and_explicit_legacy_conversion_guard() {
    use opencut_editor_core::scene::{SampledChannel, sample_animation_channel_scene};
    let mut s = Store::new();
    let mut channel = animated_gain();
    let AnimationChannel::Scalar { keys, .. } = &mut channel else {
        panic!()
    };
    keys[0].segment_to_next = ScalarSegment::Bezier;
    keys[0].tangent_mode = TangentMode::Aligned;
    keys[0].right_handle = Some(CurveHandle {
        dt: Time {
            ticks: "2".into(),
            timebase: 3,
        },
        dv: 25.,
    });
    keys[1].left_handle = Some(CurveHandle {
        dt: Time {
            ticks: "-2".into(),
            timebase: 3,
        },
        dv: -5.,
    });
    s.project.sequences[0].tracks[1].elements[0].channels = vec![channel.clone()];
    let input = accelerated(&s);
    let result = plan(&s, &input).unwrap();
    let converted = &changed(&result).channels[0];
    let AnimationChannel::Scalar { keys, .. } = converted else {
        panic!()
    };
    assert_eq!(keys[1].tangent_mode, TangentMode::Aligned);
    assert!(keys[1].left_handle.is_some());
    for i in 0..=20 {
        let time = Time {
            ticks: i.to_string(),
            timebase: 20,
        };
        let SampledChannel::Scalar { raw: a, .. } =
            sample_animation_channel_scene(&channel, &time).unwrap()
        else {
            panic!()
        };
        let SampledChannel::Scalar { raw: b, .. } =
            sample_animation_channel_scene(converted, &time).unwrap()
        else {
            panic!()
        };
        assert!((a - b).abs() < 1e-3);
    }
    let AnimationChannel::Scalar { keys, .. } =
        &mut s.project.sequences[0].tracks[1].elements[0].channels[0]
    else {
        panic!()
    };
    keys[0].segment_to_next = ScalarSegment::LegacyNormalizedBezier {
        curve: opencut_contracts::historical_visual::HistoricalVisualCubicBezier {
            x1: 0.2,
            y1: 0.,
            x2: 0.8,
            y2: 1.,
        },
    };
    assert!(
        plan(&s, &input)
            .unwrap_err()
            .message
            .contains("explicit conversion")
    );
}
#[test]
fn classic_duration_clamp_keeps_discrete_boundary_even_before_first_or_after_last_key() {
    let s = Store::new();
    let channel = AnimationChannel::Discrete {
        id: "alignment".into(),
        target: ChannelTarget::Element {
            element_id: "audio".into(),
        },
        parameter: DiscreteParameter::GraphicStrokeAlign,
        keys: vec![DiscreteKey {
            id: "align-key".into(),
            time: t(2),
            value: DiscreteValue::StrokeAlign {
                value: StrokeAlign::Outside,
            },
        }],
    };
    // Direct pure clamp fixture: target applicability is checked by the caller's
    // project validation; this exercises only the Classic discrete split rule.
    for end in [1, 3] {
        let result = crate::timeline::clamp_animation_duration_classic(
            &s.project,
            &[channel.clone()],
            &t(end),
            "head",
            "key",
            "audio",
        )
        .unwrap();
        let AnimationChannel::Discrete { keys, .. } = &result[0] else {
            panic!()
        };
        assert_eq!(keys.last().unwrap().time, t(end));
        assert_eq!(
            keys.last().unwrap().value,
            DiscreteValue::StrokeAlign {
                value: StrokeAlign::Outside
            }
        );
        assert_eq!(keys.len(), if end == 1 { 1 } else { 2 });
    }
}

fn transform_draft_fixture() -> (Store, ApplyProject) {
    let mut store = Store::new();
    let track = &mut store.project.sequences[0].tracks[1];
    track.kind = SceneTrackKind::Graphic;
    let element = &mut track.elements[0];
    element.content=serde_json::from_value(serde_json::json!({"kind":"graphic","graphic":{"source_size":{"width":100,"height":100},"fill":{"encoding":"srgb","red":1,"green":0,"blue":0,"alpha":1},"stroke":{"color":{"encoding":"srgb","red":0,"green":0,"blue":0,"alpha":1},"width_px":0,"align":"center"},"geometry":{"kind":"rectangle","corner_radius_percent":0}}})).unwrap();
    element.source_span = ElementSourceSpan::ClassicTrimmed {
        trim_start: t(0),
        trim_end: t(0),
        source_duration: None,
    };
    element.visual=Some(serde_json::from_value(serde_json::json!({"profile":"classic_canvas_v31","transform":{"position_x_px":0,"position_y_px":0,"scale_x":1,"scale_y":1,"rotation_degrees":0},"opacity":1,"blend_mode":"normal","effects":[],"masks":[]})).unwrap());
    let mut changed = element.clone();
    changed.visual.as_mut().unwrap().transform.position_x_px = 20.;
    let request = ApplyProject {
        branch_name: None,
        project_id: "p".into(),
        expected_revision: "head".into(),
        idempotency_key: "draft-original".into(),
        label: "Transform selection".into(),
        operations: vec![ProjectEditOperation::ElementPut {
            sequence_id: "s".into(),
            track_id: "sound".into(),
            element: Box::new(changed),
        }],
    };
    (store, request)
}
#[test]
fn transform_draft_compiles_only_a_read_only_fixed_head_candidate() {
    let (store, request) = transform_draft_fixture();
    let original = store.project.clone();
    let (candidate, digest) = crate::element_transform::prepare_draft(
        &store,
        &request,
        "s",
        &t(1),
        opencut_contracts::element_transform::TransformGestureIntent::Translate,
    )
    .unwrap();
    assert_eq!(candidate.revision, "head");
    assert_eq!(
        candidate.project.sequences[0].tracks[1].elements[0]
            .visual
            .as_ref()
            .unwrap()
            .transform
            .position_x_px,
        20.
    );
    assert_eq!(store.project, original);
    assert!(digest.starts_with("sha256:"));
    let (_, second) = crate::element_transform::prepare_draft(
        &store,
        &request,
        "s",
        &t(1),
        opencut_contracts::element_transform::TransformGestureIntent::Translate,
    )
    .unwrap();
    assert_eq!(digest, second);
}

#[test]
fn transform_draft_preserves_opacity_color_and_rejects_animated_translation() {
    let (mut store, mut request) = transform_draft_fixture();
    let mut opacity = animated_gain();
    let AnimationChannel::Scalar {
        id, binding, keys, ..
    } = &mut opacity
    else {
        panic!()
    };
    *id = "opacity-channel".into();
    *binding = ScalarBinding::Property {
        parameter: ScalarParameter::Opacity,
    };
    keys[0].id = "opacity-left".into();
    keys[0].value = 0.3;
    keys[1].id = "opacity-right".into();
    keys[1].value = 0.9;
    let mut color = opacity.clone();
    let AnimationChannel::Scalar {
        id, binding, keys, ..
    } = &mut color
    else {
        panic!()
    };
    *id = "color-channel".into();
    *binding = ScalarBinding::ColorComponent {
        parameter: ColorParameter::GraphicFill,
        component: ColorComponent::Red,
    };
    keys[0].id = "color-left".into();
    keys[1].id = "color-right".into();
    let channels = vec![opacity, color];
    store.project.sequences[0].tracks[1].elements[0].channels = channels.clone();
    let ProjectEditOperation::ElementPut { element, .. } = &mut request.operations[0] else {
        panic!()
    };
    element.channels = channels.clone();
    let (candidate, _) = crate::element_transform::prepare_draft(
        &store,
        &request,
        "s",
        &t(1),
        opencut_contracts::element_transform::TransformGestureIntent::Translate,
    )
    .unwrap();
    assert_eq!(
        candidate.project.sequences[0].tracks[1].elements[0].channels,
        channels
    );
    assert_eq!(store.element().channels, channels);
    let AnimationChannel::Scalar { binding, .. } =
        &mut store.project.sequences[0].tracks[1].elements[0].channels[0]
    else {
        panic!()
    };
    *binding = ScalarBinding::Property {
        parameter: ScalarParameter::PositionXPx,
    };
    let ProjectEditOperation::ElementPut { element, .. } = &mut request.operations[0] else {
        panic!()
    };
    element.channels = store.element().channels.clone();
    assert!(
        crate::element_transform::prepare_draft(
            &store,
            &request,
            "s",
            &t(1),
            opencut_contracts::element_transform::TransformGestureIntent::Translate,
        )
        .unwrap_err()
        .message
        .contains("Position animation")
    );
    let ProjectEditOperation::ElementPut { element, .. } = &request.operations[0] else {
        panic!()
    };
    assert_eq!(element.channels, store.element().channels);
}
#[test]
fn transform_draft_rejects_an_associated_valid_legacy_crossfade() {
    let (mut store, mut request) = transform_draft_fixture();
    store.project.assets[0].kind = opencut_contracts::AssetKind::Video;
    let track = &mut store.project.sequences[0].tracks[1];
    track.kind = SceneTrackKind::Video;
    let first = &mut track.elements[0];
    first.source_span = ElementSourceSpan::ClassicTrimmed {
        trim_start: t(1),
        trim_end: t(7),
        source_duration: Some(t(10)),
    };
    first.content=serde_json::from_value(serde_json::json!({"kind":"video","asset_id":"a","video_stream":null,"source_audio_enabled":true,"audio":{"stream":null,"gain":{"unit":"decibels","value":0},"muted":false},"retime":{"rate":{"numerator":1,"denominator":1},"maintain_pitch":false,"mapping":{"kind":"exact_rational"}}})).unwrap();
    let mut next = first.clone();
    next.id = "next-video".into();
    next.timeline_range.start = t(2);
    track.elements.push(next);
    store.project.sequences[0]
        .transitions
        .push(SceneTransition {
            id: "crossfade".into(),
            outgoing_element_id: "audio".into(),
            incoming_element_id: "next-video".into(),
            duration: t(1),
            transition: TransitionKind::LegacyCrossfadeV3,
        });
    opencut_editor_core::scene::validate_project_scene(&store.project).unwrap();
    let ProjectEditOperation::ElementPut { element, .. } = &mut request.operations[0] else {
        panic!()
    };
    let transform = element.visual.as_ref().unwrap().transform;
    **element = store.element().clone();
    element.visual.as_mut().unwrap().transform = transform;
    assert!(
        crate::element_transform::prepare_draft(
            &store,
            &request,
            "s",
            &t(1),
            opencut_contracts::element_transform::TransformGestureIntent::Translate
        )
        .unwrap_err()
        .message
        .contains("transition")
    );
}
#[test]
fn transform_draft_rejects_resource_or_opacity_changes_duplicate_targets_and_inactive_time() {
    for variant in ["opacity", "content", "duplicate", "time", "head", "locked"] {
        let (mut store, mut request) = transform_draft_fixture();
        if let ProjectEditOperation::ElementPut { element, .. } = &mut request.operations[0] {
            match variant {
                "opacity" => element.visual.as_mut().unwrap().opacity = 0.5,
                "content" => element.name = "Unreviewed".into(),
                _ => {}
            }
        }
        if variant == "duplicate" {
            request.operations.push(request.operations[0].clone());
        }
        if variant == "head" {
            request.expected_revision = "other".into();
        }
        if variant == "locked" {
            store.project.sequences[0].tracks[1].locked = true;
        }
        assert!(
            crate::element_transform::prepare_draft(
                &store,
                &request,
                "s",
                &t(if variant == "time" { 2 } else { 1 }),
                opencut_contracts::element_transform::TransformGestureIntent::Translate,
            )
            .is_err(),
            "{variant}"
        );
    }
}
#[test]
fn transform_draft_keeps_revocation_in_the_shared_public_planner() {
    let (mut store, request) = transform_draft_fixture();
    store.deny = true;
    assert_eq!(
        crate::element_transform::prepare_draft(
            &store,
            &request,
            "s",
            &t(1),
            opencut_contracts::element_transform::TransformGestureIntent::Translate
        )
        .unwrap_err()
        .code,
        "NOT_AUTHORIZED"
    );
}
