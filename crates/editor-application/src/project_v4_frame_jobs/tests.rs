use super::*;
use crate::{classic_visual::*, project_v4::new_document};
use opencut_contracts::{
    AssetKind, AssetVersion, CreateProject, FrameRate, LockedAsset, TimeRange, project_v4::*,
};

fn t(n: i64, d: u32) -> Time {
    Time {
        ticks: n.to_string(),
        timebase: d,
    }
}
fn policy() -> LocalVisualExecutionPolicyV1 {
    LocalVisualExecutionPolicyV1 {
        max_frame_pixels: 16_000_000,
        max_decoded_pixels: 64_000_000,
        max_gpu_bytes: 256 * 1024 * 1024,
        max_rgba_bytes: 256 * 1024 * 1024,
        max_encoded_bytes: 32 * 1024 * 1024,
        max_output_bytes: 129 * 1024 * 1024,
        max_rss_bytes: 2 * 1024 * 1024 * 1024,
        deadline_ms: 120000,
    }
}
fn input() -> PreparedV4FramePng {
    let mut project = new_document(&CreateProject {
        id: "project".into(),
        name: "Picture".into(),
        sequence_id: "scene".into(),
        width: 64,
        height: 48,
        fps: FrameRate {
            numerator: 30,
            denominator: 1,
        },
    })
    .unwrap();
    project.sequences[0].main_track_id = "main".into();
    project.sequences[0].tracks[0].id = "main".into();
    project.assets.push(AssetV4 {
        id: "image".into(),
        name: "Picture".into(),
        kind: AssetKind::Image,
        version: AssetVersion {
            id: format!("av_{}", "a".repeat(64)),
            content_hash: "a".repeat(64),
            size: 16,
        },
        duration: None,
        origin: ResourceOriginV4::LegacyLocator {
            uri: "/private/never-publish.png".into(),
        },
        metadata: MediaMetadataV4 {
            mime_type: Some("image/png".into()),
            intrinsic_size: Some(CanvasSizeV4 {
                width: 1,
                height: 1,
            }),
            frame_rate: None,
            rotation_degrees: None,
            audio: AudioPresenceV4::Absent {},
        },
        license: None,
    });
    project.sequences[0].tracks[0]
        .elements
        .push(TimelineElementV4 {
            id: "element".into(),
            name: "Picture".into(),
            timeline_range: TimeRange {
                start: t(0, 1),
                duration: t(2, 1),
            },
            source_span: ElementSourceSpanV4::ClassicTrimmed {
                trim_start: t(0, 1),
                trim_end: t(0, 1),
                source_duration: None,
            },
            enabled: true,
            content: ElementContentV4::Image {
                asset_id: "image".into(),
            },
            channels: vec![],
            visual: Some(VisualStyleV4 {
                profile: VisualProfileV4::ClassicCanvasV31,
                transform: TransformV4 {
                    position_x_px: 0.0,
                    position_y_px: 0.0,
                    scale_x: 1.0,
                    scale_y: 1.0,
                    rotation_degrees: 0.0,
                },
                opacity: 1.0,
                blend_mode: BlendModeV4::Normal,
                effects: vec![],
                masks: vec![],
            }),
        });
    PreparedV4FramePng {
        snapshot: ProjectSnapshotV4 {
            project,
            revision: "fixed-r1".into(),
        },
        sequence_id: "scene".into(),
        time: t(1, 7),
        asset_lock: AssetLockManifest {
            version: 1,
            revision: "fixed-r1".into(),
            sequence_id: "scene".into(),
            assets: vec![LockedAsset {
                asset_id: "image".into(),
                content_hash: "a".repeat(64),
                size: 16,
            }],
        },
        authority: TrustedJobAuthorityV1::LocalOwner,
        processing_policy: policy(),
    }
}
fn observation() -> DecodedVisualSourceV4 {
    DecodedVisualSourceV4::ReadyImage {
        element_id: "element".into(),
        width: 320,
        height: 90,
    }
}
fn stored(input: &PreparedV4FramePng) -> StoredV4FramePngResultV1 {
    let prepared = validate_prepared(input).unwrap();
    StoredV4FramePngResultV1 {
        public: V4FramePngResultV1 {
            schema_version: 1,
            identity: public_identity(&prepared),
            asset_lock: input.asset_lock.clone(),
            render_input_sha256: render_input_sha256(
                &input.snapshot,
                &input.sequence_id,
                &input.time,
            )
            .unwrap(),
            sources: public_sources(&prepared, &[observation()]).unwrap(),
            execution: V4FrameExecutionSummaryV1 {
                host_sha256: "b".repeat(64),
                browser_version: "test-browser".into(),
                browser_sha256: "c".repeat(64),
                module_sha256: "d".repeat(64),
                shader_sha256: "e".repeat(64),
                playwright_version: "1.63.0".into(),
                mediabunny_version: "1.41.0".into(),
                gpu_backend: "webgpu".into(),
                gpu_device: "fixture-device".into(),
                texture_format: "bgra8unorm".into(),
                decoder_profile: CLASSIC_VISUAL_DECODER_PROFILE_V4.into(),
            },
            artifact: V4FrameArtifactMetadataV1 {
                mime_type: "image/png".into(),
                size_bytes: "100".into(),
                content_hash: "f".repeat(64),
            },
        },
        artifact: ArtifactDescriptor {
            path: "/private/result.png".into(),
            mime_type: "image/png".into(),
            size: 100,
            content_hash: "f".repeat(64),
        },
    }
}
#[test]
fn exact_presentation_time_full_snapshot_identity_and_safe_result_projection() {
    let input = input();
    let prepared = validate_prepared(&input).unwrap();
    assert_eq!(prepared.identity().time, t(1, 7));
    let digest = render_input_sha256(&input.snapshot, &input.sequence_id, &input.time).unwrap();
    let mut renamed = input.snapshot.clone();
    renamed.project.name = "Other authored snapshot".into();
    assert_ne!(
        digest,
        render_input_sha256(&renamed, &input.sequence_id, &input.time).unwrap()
    );
    let result = stored(&input);
    let public = validate_stored_result(&input, &result).unwrap();
    assert_eq!(public.sources[0].width, Some(320));
    let json = serde_json::to_string(&public).unwrap();
    assert!(!json.contains("/private"));
    assert!(!json.contains("uri"));
    assert!(!json.contains("path"));
    for mutation in 0..4 {
        let mut changed = result.clone();
        match mutation {
            0 => changed.public.identity.time = t(1, 8),
            1 => changed.public.sources[0].asset_id = "other".into(),
            2 => changed.public.asset_lock.assets[0].size += 1,
            _ => changed.public.execution.shader_sha256 = "unknown".into(),
        }
        assert_eq!(
            validate_stored_result(&input, &changed).unwrap_err().code,
            "INVALID_STORED_RESULT"
        );
    }
}
#[test]
fn original_request_binding_and_lock_are_not_replaced_by_a_prepared_payload() {
    let input = input();
    let request = JobRequest {
        kind: V4_FRAME_PNG_KIND.into(),
        idempotency_key: "key".into(),
        input: serde_json::to_value(SubmitV4FramePng {
            idempotency_key: "key".into(),
            project_id: "project".into(),
            revision: "fixed-r1".into(),
            sequence_id: "scene".into(),
            time: t(1, 7),
        })
        .unwrap(),
    };
    validate_prepared_request(&request, &input).unwrap();
    let mut other = input.clone();
    other.time = t(1, 8);
    assert!(validate_prepared_request(&request, &other).is_err());
    other = input.clone();
    other.asset_lock.assets[0].content_hash = "b".repeat(64);
    assert_eq!(
        validate_prepared(&other).unwrap_err().code,
        "ASSET_LOCK_INVALID"
    );
}
#[derive(Default)]
struct Resources {
    checkpoints: usize,
}
impl ClassicVisualResources for Resources {
    fn resolve(&mut self, _: &VisualResource, _: &str) -> Result<(String, u64), EditorError> {
        Ok(("transfer".into(), 16))
    }
    fn read(&mut self, _: &str, _: u64, _: u32) -> Result<(Vec<u8>, bool), EditorError> {
        Ok((vec![0; 16], true))
    }
    fn release(&mut self, _: &str) -> Result<(), EditorError> {
        Ok(())
    }
    fn checkpoint(&mut self) -> Result<(), EditorError> {
        self.checkpoints += 1;
        Ok(())
    }
}
#[derive(Default)]
struct Sink {
    bytes: usize,
}
impl ClassicVisualRgbaSink for Sink {
    fn chunk(&mut self, offset: u64, bytes: &[u8]) -> Result<(), EditorError> {
        assert_eq!(offset, self.bytes as u64);
        self.bytes += bytes.len();
        Ok(())
    }
}
#[test]
fn session_phases_bind_once_and_check_complete_rgba_and_frozen_geometry_receipt() {
    let input = input();
    let prepared = validate_prepared(&input).unwrap();
    let mut session = ClassicVisualSession::new(&prepared, &input.processing_policy).unwrap();
    let mut resources = Resources::default();
    let mut sink = Sink::default();
    assert!(
        !serde_json::to_string(&session.plan().unwrap())
            .unwrap()
            .contains("/private")
    );
    assert!(session.rgba_chunk(0, &[0; 4], &mut sink).is_err());
    assert!(
        session
            .callback(
                VisualCallback::Bind(VisualBindFrame {
                    decoded: vec![observation()]
                }),
                &mut resources
            )
            .is_err()
    );
    let admission = session
        .callback(
            VisualCallback::Admit(VisualAdmitDecode {
                element_id: "element".into(),
            }),
            &mut resources,
        )
        .unwrap();
    assert!(admission["maxPixels"].is_string());
    assert!(
        session
            .callback(
                VisualCallback::Admit(VisualAdmitDecode {
                    element_id: "element".into()
                }),
                &mut resources
            )
            .is_err()
    );
    session
        .callback(
            VisualCallback::Bind(VisualBindFrame {
                decoded: vec![observation()],
            }),
            &mut resources,
        )
        .unwrap();
    let bound = session.bound().unwrap().clone();
    assert_eq!(bound.layers[0].quad.as_ref().unwrap().height, 18.0);
    assert!(
        session
            .callback(
                VisualCallback::Bind(VisualBindFrame {
                    decoded: vec![observation()]
                }),
                &mut resources
            )
            .is_err()
    );
    let mut receipt = ClassicVisualRenderReceipt {
        identity: prepared.identity().clone(),
        engine_identity: CLASSIC_VISUAL_PROFILE_V4.into(),
        bound,
        rgba_bytes: "12288".into(),
        peak_accounted_bytes: "12288".into(),
    };
    assert!(session.validate_receipt(&receipt).is_err());
    assert!(session.rgba_chunk(4, &[0; 4], &mut sink).is_err());
    session.rgba_chunk(0, &[0; 8192], &mut sink).unwrap();
    session.rgba_chunk(8192, &[0; 4096], &mut sink).unwrap();
    session.validate_receipt(&receipt).unwrap();
    receipt.bound.layers[0].quad.as_mut().unwrap().width += 1.0;
    assert!(session.validate_receipt(&receipt).is_err());
    assert!(resources.checkpoints >= 5);
}
#[test]
fn resource_and_actual_dimensions_cannot_bypass_admission() {
    let input = input();
    let prepared = validate_prepared(&input).unwrap();
    let mut resources = Resources::default();
    let mut session = ClassicVisualSession::new(&prepared, &input.processing_policy).unwrap();
    let mut resource = session.resource("image").unwrap();
    resource.size = "17".into();
    assert!(
        session
            .callback(
                VisualCallback::Resolve(VisualResolveBytes {
                    resource,
                    handle: "fixed-handle".into()
                }),
                &mut resources
            )
            .is_err()
    );
    let giant = DecodedVisualSourceV4::ReadyImage {
        element_id: "element".into(),
        width: 16385,
        height: 1,
    };
    assert_eq!(
        session.project_decoded_sources(&[giant]).unwrap_err().code,
        "VISUAL_BUDGET_EXCEEDED"
    );
    let mut denied = input.processing_policy.clone();
    denied.max_gpu_bytes = 1;
    assert!(ClassicVisualSession::new(&prepared, &denied).is_err());
    denied = input.processing_policy.clone();
    denied.deadline_ms = 0;
    assert!(ClassicVisualSession::new(&prepared, &denied).is_err());
}

#[test]
fn runtime_unique_resource_limit_is_checked_before_queue_execution() {
    let input = input();
    let prepared = validate_prepared(&input).unwrap();
    let mut graph = prepared.graph().clone();
    let original_layer = graph.layers[0].clone();
    let original_resource = graph.resources[0].clone();
    for index in 1..33 {
        let id = format!("image-{index}");
        let mut layer = original_layer.clone();
        layer.element_id = format!("element-{index}");
        layer.content = SampledContentV4::Image {
            asset_id: id.clone(),
        };
        graph.layers.push(layer);
        let mut resource = original_resource.clone();
        resource.asset_id = id;
        graph.resources.push(resource);
    }
    let too_many = prepare_visual_frame_v4(&graph).unwrap();
    assert_eq!(
        ClassicVisualSession::new(&too_many, &input.processing_policy)
            .err()
            .unwrap()
            .code,
        "VISUAL_BUDGET_EXCEEDED"
    );
    graph.layers.pop();
    graph.resources.pop();
    let admitted = prepare_visual_frame_v4(&graph).unwrap();
    assert!(ClassicVisualSession::new(&admitted, &input.processing_policy).is_ok());
    // The limit counts unique resources, so reusing a source does not consume a
    // second transfer slot; the separate layer bound still applies.
    let mut repeated = original_layer;
    repeated.element_id = "reused-element".into();
    graph.layers.push(repeated);
    let reused = prepare_visual_frame_v4(&graph).unwrap();
    assert!(ClassicVisualSession::new(&reused, &input.processing_policy).is_ok());
}

#[test]
fn per_layer_zero_decode_budget_is_rejected_by_shared_admission_and_callback() {
    let input = input();
    let prepared = validate_prepared(&input).unwrap();
    let mut graph = prepared.graph().clone();
    let mut second = graph.layers[0].clone();
    second.element_id = "second-element".into();
    graph.layers.push(second);
    let prepared = prepare_visual_frame_v4(&graph).unwrap();
    let mut policy = input.processing_policy;
    policy.max_decoded_pixels = 1;
    let mut session = ClassicVisualSession::new(&prepared, &policy).unwrap();
    assert_eq!(
        session.decode_admission().unwrap_err().code,
        "VISUAL_BUDGET_EXCEEDED"
    );
    assert_eq!(
        session
            .callback(
                VisualCallback::Admit(VisualAdmitDecode {
                    element_id: "element".into(),
                }),
                &mut Resources::default()
            )
            .unwrap_err()
            .code,
        "VISUAL_BUDGET_EXCEEDED"
    );
}
