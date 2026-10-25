use super::*;
use crate::scene::{SceneRead, StoredProjectRevision, public::plan_public};
use opencut_contracts::{AssetVersion, CreateProject, TransactionResult};
use std::cell::Cell;
struct Store {
    snapshot: EditorSnapshot,
    authorizations: Cell<usize>,
    reads: Cell<usize>,
    deny_after: Option<usize>,
    advance_after_read: bool,
    wrong_scope: bool,
}
impl Store {
    fn new() -> Self {
        Self {
            snapshot: EditorSnapshot {
                project: crate::scene::new_document(&CreateProject {
                    id: "project".into(),
                    name: "Graphics".into(),
                    sequence_id: "scene".into(),
                    width: 320,
                    height: 180,
                    fps: FrameRate {
                        numerator: 24,
                        denominator: 1,
                    },
                })
                .unwrap(),
                revision: "head".into(),
            },
            authorizations: Cell::new(0),
            reads: Cell::new(0),
            deny_after: None,
            advance_after_read: false,
            wrong_scope: false,
        }
    }
    fn input(&self, command: GraphicPlanCommand) -> GraphicPlanRequest {
        GraphicPlanRequest {
            project_id: "project".into(),
            expected_revision: "head".into(),
            sequence_id: "scene".into(),
            idempotency_key: "original-graphic-key".into(),
            command,
        }
    }
    fn values(&self) -> GraphicValues {
        let initial = defaults(GraphicKind::Polygon, time(0, 1));
        GraphicValues {
            name: initial.name,
            graphic: initial.graphic,
            transform: initial.transform,
            opacity: initial.opacity,
            start: GraphicTimeInput::Exact {
                time: initial.timeline_range.start,
            },
            duration: GraphicTimeInput::Exact {
                time: initial.timeline_range.duration,
            },
        }
    }
    fn create(&self) -> GraphicPlanRequest {
        self.input(GraphicPlanCommand::Create {
            element_id: "shape".into(),
            track: GraphicTrackTarget::New {
                track_id: "graphics".into(),
            },
            values: self.values(),
        })
    }
    fn with_graphic() -> Self {
        let mut store = Self::new();
        let mut track = new_track("graphics".into());
        track.elements.push(new_element(
            "shape".into(),
            &defaults(GraphicKind::Polygon, time(0, 1)),
        ));
        store.snapshot.project.sequences[0].tracks.push(track);
        store
    }
    fn edit(&self) -> GraphicPlanRequest {
        let element = &self.snapshot.project.sequences[0].tracks[1].elements[0];
        let ElementContent::Graphic { graphic } = &element.content else {
            panic!()
        };
        let visual = element.visual.as_ref().unwrap();
        self.input(GraphicPlanCommand::Edit {
            target: TimelineTarget {
                track_id: "graphics".into(),
                element_id: "shape".into(),
            },
            values: GraphicValues {
                name: element.name.clone(),
                graphic: graphic.clone(),
                transform: visual.transform.clone(),
                opacity: visual.opacity,
                start: GraphicTimeInput::Exact {
                    time: element.timeline_range.start.clone(),
                },
                duration: GraphicTimeInput::Exact {
                    time: element.timeline_range.duration.clone(),
                },
            },
        })
    }
}
impl SceneRead for Store {
    fn authorize_project(&self, _: SceneCommitIntent<'_>) -> Result<(), EditorError> {
        let count = self.authorizations.get() + 1;
        self.authorizations.set(count);
        if self.deny_after.is_some_and(|limit| count > limit) {
            Err(EditorError::new("NOT_AUTHORIZED", "Read-only or revoked"))
        } else {
            Ok(())
        }
    }
    fn head_scene_source(&self, _: &str) -> Result<StoredProjectRevision, EditorError> {
        self.reads.set(self.reads.get() + 1);
        let mut project = self.snapshot.project.clone();
        if self.wrong_scope {
            project.id = "other-project".into();
        }
        Ok(StoredProjectRevision {
            revision: if self.advance_after_read && self.reads.get() > 1 {
                "new-head".into()
            } else {
                self.snapshot.revision.clone()
            },
            document: StoredProjectDocument::Editable(Box::new(project)),
        })
    }
    fn revision_scene_source(
        &self,
        _: &str,
        _: &str,
    ) -> Result<StoredProjectRevision, EditorError> {
        unreachable!("No historical lookup or resource rebinding")
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
        unreachable!("Planner never writes receipts")
    }
}
impl ProjectPublicResources for Store {
    fn authorized_project_media_scene(
        &self,
        _: &str,
        _: &str,
        _: &AssetVersion,
    ) -> Result<ProjectAsset, EditorError> {
        unreachable!("Parametric graphic planner never resolves media")
    }
}
fn values_mut(input: &mut GraphicPlanRequest) -> &mut GraphicValues {
    match &mut input.command {
        GraphicPlanCommand::Create { values, .. } | GraphicPlanCommand::Edit { values, .. } => {
            values
        }
        _ => panic!(),
    }
}
fn element(output: &GraphicPlanResult) -> &TimelineElement {
    output
        .transaction
        .as_ref()
        .unwrap()
        .operations
        .iter()
        .find_map(|op| match op {
            ProjectEditOperation::ElementPut { element, .. } => Some(element.as_ref()),
            _ => None,
        })
        .unwrap()
}

#[test]
fn graphic_defaults_match_classic_all_four_shapes_without_write() {
    for kind in [
        GraphicKind::Rectangle,
        GraphicKind::Ellipse,
        GraphicKind::Polygon,
        GraphicKind::Star,
    ] {
        let store = Store::new();
        let result = plan(
            &store,
            &store.input(GraphicPlanCommand::Defaults {
                geometry: kind,
                start: GraphicTimeInput::DecimalSeconds {
                    value: "0.13".into(),
                },
            }),
        )
        .unwrap();
        let initial = result.initial.unwrap();
        assert_eq!(
            initial.graphic.source_size,
            CanvasSize {
                width: 512,
                height: 512
            }
        );
        assert_eq!(initial.timeline_range.start, time(1, 8));
        assert_eq!(initial.timeline_range.duration, time(5, 1));
        assert_eq!(initial.graphic.fill.red, 1.0);
        assert_eq!(initial.graphic.stroke.color.red, 0.0);
        assert_eq!(initial.graphic.stroke.width_px, 0.0);
        assert_eq!(initial.graphic.stroke.align, StrokeAlign::Center);
        assert!(result.transaction.is_none());
        assert!(result.selection.is_none());
        assert_eq!(store.authorizations.get(), 2);
    }
}
#[test]
fn graphic_create_matches_public_apply_and_exact_original_key() {
    let store = Store::new();
    let request = store.create();
    let result = plan(&store, &request).unwrap();
    let transaction = result.transaction.as_ref().unwrap();
    assert_eq!(transaction.idempotency_key, request.idempotency_key);
    assert_eq!(transaction.expected_revision, "head");
    assert_eq!(transaction.operations.len(), 2);
    let actual = plan_public(&store, transaction).unwrap();
    assert_eq!(actual.sequences[0].tracks[1].elements[0], *element(&result));
    assert_eq!(store.snapshot.project.sequences[0].tracks.len(), 1);
    assert!(actual.fonts.is_empty());
    assert!(actual.assets.is_empty());
}
#[test]
fn graphic_edit_preserves_source_trims_visual_stacks_and_track_metadata() {
    let mut store = Store::with_graphic();
    let track = &mut store.snapshot.project.sequences[0].tracks[1];
    track.opacity = 0.7;
    track.muted = true;
    let graphic = &mut track.elements[0];
    graphic.enabled = false;
    graphic.source_span = ElementSourceSpan::ClassicTrimmed {
        trim_start: time(1, 7),
        trim_end: time(2, 7),
        source_duration: None,
    };
    let style = graphic.visual.as_mut().unwrap();
    style.blend_mode = BlendMode::Multiply;
    style.effects.push(SceneEffect {
        id: "retained-blur".into(),
        enabled: false,
        effect: EffectKind::ClassicBlur { intensity: 8.0 },
    });
    let before = graphic.clone();
    let mut request = store.edit();
    let values = values_mut(&mut request);
    values.name = "Rounded pentagon".into();
    values.graphic.geometry = GraphicGeometry::Polygon {
        sides: 4.5,
        corner_radius_percent: 32.5,
    };
    values.graphic.stroke.width_px = 90.5;
    values.graphic.stroke.align = StrokeAlign::Outside;
    values.transform.position_x_px = 17.25;
    values.transform.scale_y = 0.75;
    values.opacity = 0.6;
    values.start = GraphicTimeInput::Exact { time: time(1, 3) };
    values.duration = GraphicTimeInput::Exact { time: time(19, 3) };
    let result = plan(&store, &request).unwrap();
    let actual = element(&result);
    assert_eq!(actual.source_span, before.source_span);
    assert_eq!(actual.enabled, before.enabled);
    assert_eq!(actual.channels, before.channels);
    let old = before.visual.unwrap();
    let visual = actual.visual.as_ref().unwrap();
    assert_eq!(visual.profile, old.profile);
    assert_eq!(visual.blend_mode, old.blend_mode);
    assert_eq!(visual.effects, old.effects);
    assert_eq!(visual.masks, old.masks);
    assert_eq!(visual.transform.position_x_px, 17.25);
    assert_eq!(visual.opacity, 0.6);
    assert_eq!(actual.timeline_range.start, time(1, 3));
    assert_eq!(actual.timeline_range.duration, time(19, 3));
    assert_eq!(result.transaction.unwrap().operations.len(), 1);
}
#[test]
fn graphic_equivalent_rationals_return_no_transaction() {
    let store = Store::with_graphic();
    let mut request = store.edit();
    values_mut(&mut request).duration = GraphicTimeInput::Exact { time: time(10, 2) };
    let result = plan(&store, &request).unwrap();
    assert!(result.transaction.is_none());
    assert_eq!(result.initial.unwrap().timeline_range.duration, time(5, 1));
}
#[test]
fn graphic_plan_rejects_wrong_track_lock_collision_and_overlap() {
    let mut store = Store::with_graphic();
    let edit = store.edit();
    store.snapshot.project.sequences[0].tracks[1].locked = true;
    assert!(plan(&store, &edit).is_err());
    store.snapshot.project.sequences[0].tracks[1].locked = false;
    let mut create = store.create();
    if let GraphicPlanCommand::Create {
        element_id, track, ..
    } = &mut create.command
    {
        *element_id = "second".into();
        *track = GraphicTrackTarget::Existing {
            track_id: "graphics".into(),
        };
    }
    assert!(plan(&store, &create).is_err());
    if let GraphicPlanCommand::Create { track, .. } = &mut create.command {
        *track = GraphicTrackTarget::Existing {
            track_id: store.snapshot.project.sequences[0].tracks[0].id.clone(),
        };
    }
    assert!(plan(&store, &create).is_err());
    assert!(plan(&store, &store.create()).is_err());
}
#[test]
fn graphic_fixed_scope_revocation_and_late_head_change_fail_closed() {
    for case in 0..4 {
        let mut store = Store::new();
        let mut request = store.create();
        match case {
            0 => store.deny_after = Some(1),
            1 => store.advance_after_read = true,
            2 => store.wrong_scope = true,
            _ => request.expected_revision = "stale".into(),
        };
        let error = plan(&store, &request).unwrap_err();
        assert_eq!(
            error.code,
            match case {
                0 => "NOT_AUTHORIZED",
                2 => "STORAGE_ERROR",
                _ => "REVISION_CONFLICT",
            }
        );
    }
}
#[test]
fn graphic_invalid_values_and_legacy_geometry_reject() {
    for case in 0..7 {
        let store = Store::new();
        let mut request = store.create();
        let values = values_mut(&mut request);
        match case {
            0 => {
                values.graphic.geometry = GraphicGeometry::LegacyRectangle {
                    width_px: 10.0,
                    height_px: 20.0,
                }
            }
            1 => {
                values.graphic.geometry = GraphicGeometry::Polygon {
                    sides: 2.0,
                    corner_radius_percent: 0.0,
                }
            }
            2 => values.graphic.stroke.width_px = -1.0,
            3 => values.opacity = 1.1,
            4 => values.transform.scale_x = f64::NAN,
            5 => values.graphic.source_size.width = 0,
            _ => values.duration = GraphicTimeInput::Exact { time: time(0, 1) },
        };
        assert!(plan(&store, &request).is_err());
    }
}

#[test]
fn graphic_animation_preserved_and_incompatible_shape_switch_rejected() {
    let mut store = Store::with_graphic();
    let channel = AnimationChannel::Scalar {
        id: "polygon-animation".into(),
        target: ChannelTarget::Element {
            element_id: "shape".into(),
        },
        binding: ScalarBinding::Property {
            parameter: ScalarParameter::GraphicSides,
        },
        keys: vec![ScalarKey {
            id: "polygon-key".into(),
            segment_id: None,
            time: time(0, 1),
            value: 4.5,
            left_handle: None,
            right_handle: None,
            segment_to_next: ScalarSegment::Linear,
            tangent_mode: TangentMode::Auto,
        }],
        extrapolation: Extrapolation {
            before: ExtrapolationMode::Hold,
            after: ExtrapolationMode::Hold,
        },
    };
    store.snapshot.project.sequences[0].tracks[1].elements[0]
        .channels
        .push(channel.clone());
    let mut request = store.edit();
    values_mut(&mut request).graphic.fill.alpha = 0.5;
    let result = plan(&store, &request).unwrap();
    assert_eq!(element(&result).channels, vec![channel]);
    values_mut(&mut request).graphic.geometry = GraphicGeometry::Ellipse;
    let error = plan(&store, &request).unwrap_err();
    assert!(
        error.message.contains("applicable")
            || error.message.contains("type")
            || error.message.contains("binding"),
        "{}",
        error.message
    );
}
