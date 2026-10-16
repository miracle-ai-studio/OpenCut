use super::*;
use crate::project_v4::{ProjectV4Read, StoredProjectRevisionV4, public::plan_public};
use opencut_contracts::{AssetVersion, CreateProject, TransactionResult};
use std::cell::Cell;
struct Store {
    snapshot: ProjectSnapshotV4,
    authorizations: Cell<usize>,
    reads: Cell<usize>,
    deny_after: Option<usize>,
    advance_after_read: bool,
    wrong_scope: bool,
}
impl Store {
    fn new() -> Self {
        Self {
            snapshot: ProjectSnapshotV4 {
                project: crate::project_v4::new_document(&CreateProject {
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
    fn input(&self, command: GraphicPlanCommandV1) -> GraphicPlanRequestV1 {
        GraphicPlanRequestV1 {
            project_id: "project".into(),
            expected_revision: "head".into(),
            sequence_id: "scene".into(),
            idempotency_key: "original-graphic-key".into(),
            command,
        }
    }
    fn values(&self) -> GraphicValuesV1 {
        let initial = defaults(GraphicKindV1::Polygon, time(0, 1));
        GraphicValuesV1 {
            name: initial.name,
            graphic: initial.graphic,
            transform: initial.transform,
            opacity: initial.opacity,
            start: GraphicTimeInputV1::Exact {
                time: initial.timeline_range.start,
            },
            duration: GraphicTimeInputV1::Exact {
                time: initial.timeline_range.duration,
            },
        }
    }
    fn create(&self) -> GraphicPlanRequestV1 {
        self.input(GraphicPlanCommandV1::Create {
            element_id: "shape".into(),
            track: GraphicTrackTargetV1::New {
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
            &defaults(GraphicKindV1::Polygon, time(0, 1)),
        ));
        store.snapshot.project.sequences[0].tracks.push(track);
        store
    }
    fn edit(&self) -> GraphicPlanRequestV1 {
        let element = &self.snapshot.project.sequences[0].tracks[1].elements[0];
        let ElementContentV4::Graphic { graphic } = &element.content else {
            panic!()
        };
        let visual = element.visual.as_ref().unwrap();
        self.input(GraphicPlanCommandV1::Edit {
            target: TimelineTargetV4 {
                track_id: "graphics".into(),
                element_id: "shape".into(),
            },
            values: GraphicValuesV1 {
                name: element.name.clone(),
                graphic: graphic.clone(),
                transform: visual.transform.clone(),
                opacity: visual.opacity,
                start: GraphicTimeInputV1::Exact {
                    time: element.timeline_range.start.clone(),
                },
                duration: GraphicTimeInputV1::Exact {
                    time: element.timeline_range.duration.clone(),
                },
            },
        })
    }
}
impl ProjectV4Read for Store {
    fn authorize_v4(&self, _: V4Intent<'_>) -> Result<(), EditorError> {
        let count = self.authorizations.get() + 1;
        self.authorizations.set(count);
        if self.deny_after.is_some_and(|limit| count > limit) {
            Err(EditorError::new("NOT_AUTHORIZED", "Read-only or revoked"))
        } else {
            Ok(())
        }
    }
    fn head_v4_source(&self, _: &str) -> Result<StoredProjectRevisionV4, EditorError> {
        self.reads.set(self.reads.get() + 1);
        let mut project = self.snapshot.project.clone();
        if self.wrong_scope {
            project.id = "other-project".into();
        }
        Ok(StoredProjectRevisionV4 {
            revision: if self.advance_after_read && self.reads.get() > 1 {
                "new-head".into()
            } else {
                self.snapshot.revision.clone()
            },
            document: StoredProjectDocumentV4::V4(Box::new(project)),
        })
    }
    fn revision_v4_source(&self, _: &str, _: &str) -> Result<StoredProjectRevisionV4, EditorError> {
        unreachable!("No historical lookup or resource rebinding")
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
        unreachable!("Planner never writes receipts")
    }
}
impl ProjectV4PublicResources for Store {
    fn authorized_project_media_v4(
        &self,
        _: &str,
        _: &str,
        _: &AssetVersion,
    ) -> Result<AssetV4, EditorError> {
        unreachable!("Parametric graphic planner never resolves media")
    }
}
fn values_mut(input: &mut GraphicPlanRequestV1) -> &mut GraphicValuesV1 {
    match &mut input.command {
        GraphicPlanCommandV1::Create { values, .. } | GraphicPlanCommandV1::Edit { values, .. } => {
            values
        }
        _ => panic!(),
    }
}
fn element(output: &GraphicPlanResultV1) -> &TimelineElementV4 {
    output
        .transaction
        .as_ref()
        .unwrap()
        .operations
        .iter()
        .find_map(|op| match op {
            PublicProjectOperationV4::ElementPut { element, .. } => Some(element.as_ref()),
            _ => None,
        })
        .unwrap()
}

#[test]
fn graphic_defaults_match_classic_all_four_shapes_without_write() {
    for kind in [
        GraphicKindV1::Rectangle,
        GraphicKindV1::Ellipse,
        GraphicKindV1::Polygon,
        GraphicKindV1::Star,
    ] {
        let store = Store::new();
        let result = plan(
            &store,
            &store.input(GraphicPlanCommandV1::Defaults {
                geometry: kind,
                start: GraphicTimeInputV1::DecimalSeconds {
                    value: "0.13".into(),
                },
            }),
        )
        .unwrap();
        let initial = result.initial.unwrap();
        assert_eq!(
            initial.graphic.source_size,
            CanvasSizeV4 {
                width: 512,
                height: 512
            }
        );
        assert_eq!(initial.timeline_range.start, time(1, 8));
        assert_eq!(initial.timeline_range.duration, time(5, 1));
        assert_eq!(initial.graphic.fill.red, 1.0);
        assert_eq!(initial.graphic.stroke.color.red, 0.0);
        assert_eq!(initial.graphic.stroke.width_px, 0.0);
        assert_eq!(initial.graphic.stroke.align, StrokeAlignV4::Center);
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
    graphic.source_span = ElementSourceSpanV4::ClassicTrimmed {
        trim_start: time(1, 7),
        trim_end: time(2, 7),
        source_duration: None,
    };
    let style = graphic.visual.as_mut().unwrap();
    style.blend_mode = BlendModeV4::Multiply;
    style.effects.push(EffectV4 {
        id: "retained-blur".into(),
        enabled: false,
        effect: EffectKindV4::ClassicBlur { intensity: 8.0 },
    });
    let before = graphic.clone();
    let mut request = store.edit();
    let values = values_mut(&mut request);
    values.name = "Rounded pentagon".into();
    values.graphic.geometry = GraphicGeometryV4::Polygon {
        sides: 4.5,
        corner_radius_percent: 32.5,
    };
    values.graphic.stroke.width_px = 90.5;
    values.graphic.stroke.align = StrokeAlignV4::Outside;
    values.transform.position_x_px = 17.25;
    values.transform.scale_y = 0.75;
    values.opacity = 0.6;
    values.start = GraphicTimeInputV1::Exact { time: time(1, 3) };
    values.duration = GraphicTimeInputV1::Exact { time: time(19, 3) };
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
    values_mut(&mut request).duration = GraphicTimeInputV1::Exact { time: time(10, 2) };
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
    if let GraphicPlanCommandV1::Create {
        element_id, track, ..
    } = &mut create.command
    {
        *element_id = "second".into();
        *track = GraphicTrackTargetV1::Existing {
            track_id: "graphics".into(),
        };
    }
    assert!(plan(&store, &create).is_err());
    if let GraphicPlanCommandV1::Create { track, .. } = &mut create.command {
        *track = GraphicTrackTargetV1::Existing {
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
                values.graphic.geometry = GraphicGeometryV4::LegacyRectangle {
                    width_px: 10.0,
                    height_px: 20.0,
                }
            }
            1 => {
                values.graphic.geometry = GraphicGeometryV4::Polygon {
                    sides: 2.0,
                    corner_radius_percent: 0.0,
                }
            }
            2 => values.graphic.stroke.width_px = -1.0,
            3 => values.opacity = 1.1,
            4 => values.transform.scale_x = f64::NAN,
            5 => values.graphic.source_size.width = 0,
            _ => values.duration = GraphicTimeInputV1::Exact { time: time(0, 1) },
        };
        assert!(plan(&store, &request).is_err());
    }
}

#[test]
fn graphic_animation_preserved_and_incompatible_shape_switch_rejected() {
    let mut store = Store::with_graphic();
    let channel = AnimationChannelV4::Scalar {
        id: "polygon-animation".into(),
        target: ChannelTargetV4::Element {
            element_id: "shape".into(),
        },
        binding: ScalarBindingV4::Property {
            parameter: ScalarParameterV4::GraphicSides,
        },
        keys: vec![ScalarKeyV4 {
            id: "polygon-key".into(),
            segment_id: None,
            time: time(0, 1),
            value: 4.5,
            left_handle: None,
            right_handle: None,
            segment_to_next: ScalarSegmentV4::Linear,
            tangent_mode: TangentModeV4::Auto,
        }],
        extrapolation: ExtrapolationV4 {
            before: ExtrapolationModeV4::Hold,
            after: ExtrapolationModeV4::Hold,
        },
    };
    store.snapshot.project.sequences[0].tracks[1].elements[0]
        .channels
        .push(channel.clone());
    let mut request = store.edit();
    values_mut(&mut request).graphic.fill.alpha = 0.5;
    let result = plan(&store, &request).unwrap();
    assert_eq!(element(&result).channels, vec![channel]);
    values_mut(&mut request).graphic.geometry = GraphicGeometryV4::Ellipse;
    let error = plan(&store, &request).unwrap_err();
    assert!(
        error.message.contains("applicable")
            || error.message.contains("type")
            || error.message.contains("binding"),
        "{}",
        error.message
    );
}
