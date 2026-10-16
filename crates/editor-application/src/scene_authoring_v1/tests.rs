use super::*;
use crate::project_v4::{ProjectV4Read, StoredProjectRevisionV4};
use opencut_contracts::project_v4_ops::{CopyV4Project, CreateV4Project};
use opencut_contracts::{AssetVersion, CreateProject, FrameRate, Time, TransactionResult};
use std::cell::Cell;

struct Store {
    project: ProjectDocumentV4,
    authorizations: Cell<usize>,
    reads: Cell<usize>,
    revoke: Option<usize>,
    advance: bool,
    wrong_scope: bool,
}
fn time(ticks: &str) -> Time {
    Time {
        ticks: ticks.into(),
        timebase: 1,
    }
}
impl Store {
    fn new() -> Self {
        let mut project = crate::project_v4::new_document(&CreateProject {
            id: "project".into(),
            name: "Mask bounds".into(),
            sequence_id: "scene".into(),
            width: 320,
            height: 180,
            fps: FrameRate {
                numerator: 30,
                denominator: 1,
            },
        })
        .unwrap();
        project.assets.push(serde_json::from_value(serde_json::json!({
            "id":"asset", "name":"Image", "kind":"image",
            "version":{"id":format!("av_{}", "a".repeat(64)),"content_hash":"a".repeat(64),"size":12},
            "duration":null,"origin":{"kind":"legacy_locator","uri":"opencut-media://media_a"},
            "metadata":{"mime_type":"image/png","intrinsic_size":{"width":640,"height":360},"frame_rate":null,"rotation_degrees":0,"audio":{"status":"absent"}},
            "license":null
        })).unwrap());
        project.sequences[0].tracks[0].elements.push(serde_json::from_value(serde_json::json!({
            "id":"target","name":"Image","enabled":true,
            "timeline_range":{"start":{"ticks":"0","timebase":1},"duration":{"ticks":"3","timebase":1}},
            "source_span":{"profile":"classic_trimmed","trim_start":{"ticks":"0","timebase":1},"trim_end":{"ticks":"0","timebase":1},"source_duration":null},
            "content":{"kind":"image","asset_id":"asset"},"channels":[],
            "visual":{"profile":"classic_canvas_v31","transform":{"position_x_px":7,"position_y_px":-4,"scale_x":-0.5,"scale_y":1,"rotation_degrees":90},
            "opacity":0.5,"blend_mode":"multiply","effects":[],"masks":[]}
        })).unwrap());
        project.sequences[0].tracks[0].opacity = 0.8;
        Self {
            project,
            authorizations: Cell::new(0),
            reads: Cell::new(0),
            revoke: None,
            advance: false,
            wrong_scope: false,
        }
    }
    fn input(&self) -> SceneGeometryRequestV1 {
        SceneGeometryRequestV1 {
            project_id: "project".into(),
            expected_revision: "head".into(),
            sequence_id: "scene".into(),
            element_id: "target".into(),
            time: time("0"),
        }
    }
    fn target(&mut self) -> &mut TimelineElementV4 {
        &mut self.project.sequences[0].tracks[0].elements[0]
    }
}
impl ProjectV4Read for Store {
    fn authorize_v4(&self, intent: V4Intent<'_>) -> Result<(), EditorError> {
        assert!(matches!(intent, V4Intent::PublicApply(_)));
        self.authorizations.set(self.authorizations.get() + 1);
        if self
            .revoke
            .is_some_and(|limit| self.authorizations.get() > limit)
        {
            Err(EditorError::new("NOT_AUTHORIZED", "Revoked"))
        } else {
            Ok(())
        }
    }
    fn head_v4_source(&self, _: &str) -> Result<StoredProjectRevisionV4, EditorError> {
        self.reads.set(self.reads.get() + 1);
        let mut project = self.project.clone();
        if self.wrong_scope {
            project.id = "other".into();
        }
        Ok(StoredProjectRevisionV4 {
            document: StoredProjectDocumentV4::V4(Box::new(project)),
            revision: if self.advance && self.reads.get() > 1 {
                "advanced"
            } else {
                "head"
            }
            .into(),
        })
    }
    fn revision_v4_source(&self, _: &str, _: &str) -> Result<StoredProjectRevisionV4, EditorError> {
        unreachable!("No historical fallback")
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
        unreachable!("No write or receipt")
    }
}
impl ProjectV4PublicResources for Store {
    fn authorized_project_media_v4(
        &self,
        _: &str,
        _: &str,
        _: &AssetVersion,
    ) -> Result<AssetV4, EditorError> {
        unreachable!("Fixed resource only; no media capture")
    }
}

#[test]
fn complete_stack_preserves_geometry_and_source_without_execution() {
    let mut store = Store::new();
    let visual = store.target().visual.as_mut().unwrap();
    visual.effects.push(EffectV4 {
        id: "blur".into(),
        enabled: true,
        effect: EffectKindV4::ClassicBlur { intensity: 15. },
    });
    for (id, inverted) in [("one", false), ("two", true)] {
        visual.masks.push(MaskV4 {
            id: id.into(),
            enabled: true,
            inverted,
            shape: MaskShapeV4::Split {
                center_x: 0.1,
                center_y: -0.2,
                rotation_degrees: 20.,
            },
            feather: MaskFeatherV4::ClassicPixels { value: 12. },
            stroke: StrokeV4 {
                width_px: 3.,
                align: StrokeAlignV4::Outside,
                color: ColorV4 {
                    encoding: ColorEncodingV4::Srgb,
                    red: 1.,
                    green: 0.,
                    blue: 0.,
                    alpha: 0.5,
                },
            },
        });
    }
    let before = store.project.clone();
    let result = plan(&store, &store.input()).unwrap();
    assert_eq!((result.quad.width, result.quad.height), (160., 180.));
    assert!(result.quad.flip_x);
    assert_eq!(result.default_geometry.width, 0.6);
    assert_eq!(store.project, before);
    assert_eq!((store.authorizations.get(), store.reads.get()), (2, 2));
}
#[test]
fn geometry_retains_double_precision_and_observed_upright_extent() {
    let mut store = Store::new();
    store.project.assets[0].metadata.rotation_degrees = Some(90.);
    store.target().visual.as_mut().unwrap().transform.scale_x = 0.500000003;
    let result = plan(&store, &store.input()).unwrap();
    assert_eq!(result.quad.width, 101.25 * 0.500000003);
    assert_ne!(result.quad.width, f64::from(result.quad.width as f32));
    store.project.assets[0].metadata.intrinsic_size = None;
    assert_eq!(
        plan(&store, &store.input()).unwrap_err().code,
        "UNSUPPORTED_VISUAL_FEATURE"
    );
}
#[test]
fn stale_authorization_locked_and_inactive_reads_do_not_publish_geometry() {
    let mut store = Store::new();
    store.revoke = Some(1);
    assert_eq!(
        plan(&store, &store.input()).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
    let mut store = Store::new();
    store.advance = true;
    assert_eq!(
        plan(&store, &store.input()).unwrap_err().code,
        "REVISION_CONFLICT"
    );
    let mut store = Store::new();
    store.project.sequences[0].tracks[0].locked = true;
    assert_eq!(
        plan(&store, &store.input()).unwrap_err().code,
        "TRACK_LOCKED"
    );
    let store = Store::new();
    let mut input = store.input();
    input.time = time("3");
    assert_eq!(
        plan(&store, &input).unwrap_err().code,
        "UNSUPPORTED_VISUAL_FEATURE"
    );
}

#[test]
fn animated_targets_cannot_publish_static_canvas_bounds() {
    let mut store = Store::new();
    store.target().channels.push(serde_json::from_value(serde_json::json!({"kind":"scalar","id":"animation","target":{"kind":"element","element_id":"target"},
      "binding":{"kind":"property","parameter":"position_x_px"},"keys":[{"id":"key","time":{"ticks":"0","timebase":1},"value":0,"left_handle":null,"right_handle":null,"segment_to_next":{"kind":"linear"},"tangent_mode":"auto"}],"extrapolation":{"before":"hold","after":"hold"}})).unwrap());
    assert_eq!(
        plan(&store, &store.input()).unwrap_err().code,
        "UNSUPPORTED_VISUAL_FEATURE"
    );
    assert_eq!(store.authorizations.get(), 1);
}
