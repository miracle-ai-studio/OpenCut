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
    fn input(&self) -> MaskGeometryRequestV1 {
        MaskGeometryRequestV1 {
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
fn logical_quad_uses_contain_signed_transform_and_short_side_default() {
    let store = Store::new();
    let before = store.project.clone();
    let result = plan(&store, &store.input()).unwrap();
    assert_eq!((result.quad.center_x, result.quad.center_y), (167., 86.));
    assert_eq!((result.quad.width, result.quad.height), (160., 180.));
    assert!(result.quad.flip_x && !result.quad.flip_y);
    assert_eq!(result.quad.rotation_radians, std::f64::consts::FRAC_PI_2);
    assert_eq!(result.quad.opacity, 0.4);
    assert_eq!(result.default_geometry.width, 0.6);
    assert_eq!(result.default_geometry.height, 96. / 180.);
    assert_eq!(store.project, before);
    assert_eq!((store.authorizations.get(), store.reads.get()), (2, 2));
}

#[test]
fn logical_quad_preserves_double_precision_before_mask_pixel_rounding() {
    let mut store = Store::new();
    let transform = &mut store.target().visual.as_mut().unwrap().transform;
    transform.position_x_px = 0.500000003;
    transform.scale_x = 0.499999999;
    transform.rotation_degrees = 12.123456789;
    let result = plan(&store, &store.input()).unwrap();
    assert_eq!(result.quad.center_x, 160.500000003);
    assert_eq!(result.quad.width, 320.0 * 0.499999999);
    assert_ne!(result.quad.width, f64::from(result.quad.width as f32));
    assert_eq!(
        result.quad.rotation_radians,
        12.123456789 * std::f64::consts::PI / 180.0
    );
}

#[test]
fn media_quarter_turns_swap_trusted_extent_missing_or_arbitrary_orientation_refuses() {
    for rotation in [90., -90., 270., 450.] {
        let mut store = Store::new();
        store.project.assets[0].metadata.rotation_degrees = Some(rotation);
        let result = plan(&store, &store.input()).unwrap();
        assert_eq!((result.quad.width, result.quad.height), (50.625, 180.));
    }
    let mut store = Store::new();
    store.project.assets[0].metadata.rotation_degrees = Some(45.);
    assert_eq!(
        plan(&store, &store.input()).unwrap_err().code,
        "UNSUPPORTED_VISUAL_FEATURE"
    );
    store.project.assets[0].metadata.rotation_degrees = None;
    store.project.assets[0].metadata.intrinsic_size = None;
    assert_eq!(
        plan(&store, &store.input()).unwrap_err().code,
        "UNSUPPORTED_VISUAL_FEATURE"
    );
}

#[test]
fn graphic_logical_source_excludes_stroke_padding() {
    let mut store = Store::new();
    let mut track = store.project.sequences[0].tracks[0].clone();
    track.id = "graphics".into();
    track.kind = TrackKindV4::Graphic;
    track.elements[0].content = serde_json::from_value(serde_json::json!({"kind":"graphic","graphic":{
        "geometry":{"kind":"rectangle","corner_radius_percent":20}, "source_size":{"width":80,"height":40},
        "fill":{"encoding":"srgb","red":1,"green":0,"blue":0,"alpha":1},
        "stroke":{"color":{"encoding":"srgb","red":1,"green":1,"blue":1,"alpha":1},"width_px":40,"align":"outside"}
    }})).unwrap();
    store.project.sequences[0].tracks[0].elements.clear();
    store.project.sequences[0].tracks.push(track);
    let result = plan(&store, &store.input()).unwrap();
    assert_eq!((result.quad.width, result.quad.height), (160., 160.));
    assert_eq!(result.default_geometry.width, 0.6);
}

#[test]
fn fixed_head_authorization_and_publication_fences_are_read_only() {
    let mut store = Store::new();
    store.revoke = Some(0);
    assert_eq!(
        plan(&store, &store.input()).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(store.reads.get(), 0);
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
    store.wrong_scope = true;
    assert_eq!(
        plan(&store, &store.input()).unwrap_err().code,
        "STORAGE_ERROR"
    );
    let store = Store::new();
    let mut input = store.input();
    input.expected_revision = "old".into();
    assert_eq!(plan(&store, &input).unwrap_err().code, "REVISION_CONFLICT");
}

#[test]
fn locked_inactive_animated_and_unsupported_mask_targets_refuse() {
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
    let mut store = Store::new();
    store.target().visual.as_mut().unwrap().transform.scale_x = 0.;
    assert_eq!(
        plan(&store, &store.input()).unwrap_err().code,
        "UNSUPPORTED_VISUAL_FEATURE"
    );
    let mut store = Store::new();
    store.target().channels.push(serde_json::from_value(serde_json::json!({"kind":"scalar","id":"animation","target":{"kind":"element","element_id":"target"},
      "binding":{"kind":"property","parameter":"position_x_px"},"keys":[{"id":"key","time":{"ticks":"0","timebase":1},"value":0,"left_handle":null,"right_handle":null,"segment_to_next":{"kind":"linear"},"tangent_mode":"auto"}],"extrapolation":{"before":"hold","after":"hold"}})).unwrap());
    assert_eq!(
        plan(&store, &store.input()).unwrap_err().code,
        "UNSUPPORTED_VISUAL_FEATURE"
    );
    for changes in ["inverted", "stroke", "legacy", "multiple"] {
        let mut store = Store::new();
        let mut mask: MaskV4=serde_json::from_value(serde_json::json!({"id":"mask","enabled":true,"inverted":false,
          "shape":{"kind":"ellipse","geometry":{"center_x":0.1,"center_y":0.2,"width":0.5,"height":0.4,"rotation_degrees":20,"scale":2}},
          "feather":{"unit":"classic_pixels","value":8},"stroke":{"color":{"encoding":"srgb","red":1,"green":1,"blue":1,"alpha":1},"width_px":0,"align":"center"}})).unwrap();
        match changes {
            "inverted" => mask.inverted = true,
            "stroke" => mask.stroke.width_px = 1.,
            "legacy" => mask.feather = MaskFeatherV4::LegacyCanvasFraction { value: 0.1 },
            _ => {}
        }
        let masks = &mut store.target().visual.as_mut().unwrap().masks;
        masks.push(mask.clone());
        if changes == "multiple" {
            mask.id = "second-mask".into();
            masks.push(mask);
        }
        assert_eq!(
            plan(&store, &store.input()).unwrap_err().code,
            "UNSUPPORTED_VISUAL_FEATURE",
            "{changes}"
        );
    }
}
