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
        let mut project = crate::scene::new_document(&CreateProject {
            id: "project".into(),
            name: "Text authoring".into(),
            sequence_id: "scene".into(),
            width: 320,
            height: 180,
            fps: FrameRate {
                numerator: 24,
                denominator: 1,
            },
        })
        .unwrap();
        project.fonts.push(FontResource {
            id: "font".into(),
            family: "Locked face".into(),
            weight: 400,
            style: FontStyle::Normal,
            face_index: 0,
            content: AssetVersion {
                id: format!("av_{}", "a".repeat(64)),
                content_hash: "a".repeat(64),
                size: 100,
            },
            origin: ResourceOrigin::LegacyLocator {
                uri: "/private/font.ttf".into(),
            },
            license: None,
        });
        Self {
            snapshot: EditorSnapshot {
                project,
                revision: "head".into(),
            },
            authorizations: Cell::new(0),
            reads: Cell::new(0),
            deny_after: None,
            advance_after_read: false,
            wrong_scope: false,
        }
    }
    fn input(&self, command: TextPlanCommand) -> TextPlanRequest {
        TextPlanRequest {
            project_id: "project".into(),
            expected_revision: "head".into(),
            sequence_id: "scene".into(),
            idempotency_key: "original-text-key".into(),
            command,
        }
    }
    fn values(&self) -> TextValues {
        let initial = defaults(&self.snapshot.project.fonts[0], time(0, 1));
        TextValues {
            name: initial.name,
            text: initial.text,
            start: TextTimeInput::Exact { time: time(0, 1) },
            duration: TextTimeInput::Exact { time: time(5, 1) },
        }
    }
    fn create(&self) -> TextPlanRequest {
        self.input(TextPlanCommand::Create {
            element_id: "title".into(),
            track: TextTrackTarget::New {
                track_id: "titles".into(),
            },
            values: self.values(),
        })
    }
    fn with_title() -> Self {
        let mut result = Self::new();
        let initial = defaults(&result.snapshot.project.fonts[0], time(0, 1));
        let mut track = new_track("titles".into());
        track.elements.push(new_element("title".into(), &initial));
        result.snapshot.project.sequences[0].tracks.push(track);
        result
    }
    fn edit(&self) -> TextPlanRequest {
        let element = &self.snapshot.project.sequences[0].tracks[1].elements[0];
        let ElementContent::Text { text } = &element.content else {
            panic!()
        };
        self.input(TextPlanCommand::Edit {
            target: TimelineTarget {
                track_id: "titles".into(),
                element_id: "title".into(),
            },
            values: TextValues {
                name: element.name.clone(),
                text: text.clone(),
                start: TextTimeInput::Exact {
                    time: element.timeline_range.start.clone(),
                },
                duration: TextTimeInput::Exact {
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
        unreachable!("Font is already bound to the fixed head")
    }
}
fn values_mut(input: &mut TextPlanRequest) -> &mut TextValues {
    match &mut input.command {
        TextPlanCommand::Create { values, .. } | TextPlanCommand::Edit { values, .. } => values,
        _ => panic!(),
    }
}
fn element(output: &TextPlanResult) -> &TimelineElement {
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
fn defaults_match_classic_creation_and_return_no_write() {
    let mut store = Store::new();
    store.snapshot.project.fonts[0].weight = 700;
    store.snapshot.project.fonts[0].style = FontStyle::Italic;
    let output = plan(
        &store,
        &store.input(TextPlanCommand::Defaults {
            font_id: "font".into(),
            start: TextTimeInput::Exact {
                time: time(1001, 30000),
            },
        }),
    )
    .unwrap();
    let initial = output.initial.unwrap();
    assert_eq!(initial.name, "Text");
    assert_eq!(initial.text.content, "Default text");
    assert_eq!(
        initial.timeline_range,
        TimeRange {
            start: time(1001, 30000),
            duration: time(5, 1)
        }
    );
    assert_eq!(
        initial.text.font_size,
        FontSize::ClassicHeight90 { value: 15.0 }
    );
    assert_eq!(initial.text.line_height_multiplier, 1.2);
    assert_eq!(initial.text.letter_spacing_px, 0.0);
    assert_eq!(initial.text.background.padding_x_at_size15, 30.0);
    assert_eq!(initial.text.background.padding_y_at_size15, 42.0);
    assert!(!initial.text.background.enabled);
    assert_eq!(initial.text.font_weight, FontWeight::Bold);
    assert_eq!(initial.text.font_style, FontStyle::Italic);
    assert!(output.transaction.is_none());
    assert!(output.selection.is_none());
    assert_eq!(store.authorizations.get(), 2);
    assert_eq!(store.reads.get(), 2);
}

#[test]
fn create_is_one_original_public_transaction_and_preserves_existing_project() {
    let store = Store::new();
    let original = store.snapshot.clone();
    let input = store.create();
    let output = plan(&store, &input).unwrap();
    let tx = output.transaction.as_ref().unwrap();
    assert_eq!(tx.idempotency_key, input.idempotency_key);
    assert_eq!(tx.expected_revision, "head");
    assert_eq!(tx.operations.len(), 2);
    let proposed = plan_public(&store, tx).unwrap();
    let title = &proposed.sequences[0].tracks[1].elements[0];
    assert_eq!(title, element(&output));
    assert_eq!(title.visual.as_ref().unwrap().transform.scale_x, 1.0);
    assert_eq!(
        title.source_span,
        ElementSourceSpan::ClassicTrimmed {
            trim_start: time(0, 1),
            trim_end: time(0, 1),
            source_duration: None
        }
    );
    assert_eq!(store.snapshot, original);
    assert!(
        !serde_json::to_string(&output)
            .unwrap()
            .contains("/private/")
    );
}

#[test]
fn decimal_times_use_integer_frame_quantization_and_return_actual_proposal() {
    let store = Store::new();
    let mut input = store.create();
    values_mut(&mut input).start = TextTimeInput::DecimalSeconds {
        value: " 0.051000000 ".into(),
    };
    values_mut(&mut input).duration = TextTimeInput::DecimalSeconds {
        value: "1.234567891".into(),
    };
    let output = plan(&store, &input).unwrap();
    assert_eq!(
        output.initial.as_ref().unwrap().timeline_range,
        TimeRange {
            start: time(1, 24),
            duration: time(5, 4)
        }
    );
    assert_eq!(
        element(&output).timeline_range,
        output.initial.as_ref().unwrap().timeline_range
    );
    assert_eq!(
        decimal("9223372036854775807.000000000").unwrap(),
        time(i64::MAX, 1)
    );
    assert_eq!(decimal("0.000000001").unwrap(), time(1, 1_000_000_000));
}

#[test]
fn invalid_decimal_and_exact_times_fail_without_a_transaction() {
    let store = Store::new();
    for bad in [
        "",
        "-1",
        "+1",
        "NaN",
        "1e3",
        ".5",
        "1.2.3",
        "0.0000000001",
        "１２",
        "9223372036854775808",
    ] {
        assert!(decimal(bad).is_err(), "{bad}");
    }
    for bad in [time(-1, 1), time(1, 0)] {
        let mut input = store.create();
        values_mut(&mut input).start = TextTimeInput::Exact { time: bad };
        assert!(plan(&store, &input).is_err());
    }
    let mut input = store.create();
    values_mut(&mut input).duration = TextTimeInput::DecimalSeconds {
        value: "0.00001".into(),
    };
    assert!(plan(&store, &input).is_err());
}

#[test]
fn create_rejects_global_id_collisions_instead_of_replacing_entities() {
    let store = Store::with_title();
    let original = store.snapshot.clone();
    for collision in ["project", "scene", "font", "titles", "title"] {
        let mut input = store.create();
        if let TextPlanCommand::Create {
            element_id, track, ..
        } = &mut input.command
        {
            *element_id = collision.into();
            *track = TextTrackTarget::New {
                track_id: "new-text-track".into(),
            };
        }
        assert!(plan(&store, &input).is_err(), "{collision}");
    }
    let mut input = store.create();
    if let TextPlanCommand::Create { element_id, .. } = &mut input.command {
        *element_id = "new-title".into();
    }
    assert!(plan(&store, &input).is_err()); // Existing track cannot be silently replaced by New.
    assert_eq!(store.snapshot, original);
}

#[test]
fn existing_track_placement_is_explicit_rejects_overlap_and_accepts_touching_edges() {
    let store = Store::with_title();
    let mut input = store.create();
    if let TextPlanCommand::Create {
        element_id, track, ..
    } = &mut input.command
    {
        *element_id = "second".into();
        *track = TextTrackTarget::Existing {
            track_id: "titles".into(),
        };
    }
    assert!(
        plan(&store, &input)
            .unwrap_err()
            .message
            .contains("overlap")
    );
    values_mut(&mut input).start = TextTimeInput::Exact { time: time(5, 1) };
    let output = plan(&store, &input).unwrap();
    assert_eq!(output.transaction.as_ref().unwrap().operations.len(), 1);
    assert_eq!(element(&output).timeline_range.start, time(5, 1));
}

#[test]
fn edit_preserves_unexposed_visual_source_and_animation_payload() {
    let mut store = Store::with_title();
    let title = &mut store.snapshot.project.sequences[0].tracks[1].elements[0];
    title.visual.as_mut().unwrap().transform.rotation_degrees = 37.0;
    title.visual.as_mut().unwrap().transform.scale_y = 1.7;
    title.visual.as_mut().unwrap().opacity = 0.75;
    title.channels.push(AnimationChannel::Scalar {
        id: "opacity-channel".into(),
        target: ChannelTarget::Element {
            element_id: "title".into(),
        },
        binding: ScalarBinding::Property {
            parameter: ScalarParameter::Opacity,
        },
        keys: vec![ScalarKey {
            id: "opacity-key".into(),
            time: time(0, 1),
            value: 0.5,
            left_handle: None,
            right_handle: None,
            tangent_mode: TangentMode::Broken,
            segment_id: None,
            segment_to_next: ScalarSegment::Linear,
        }],
        extrapolation: Extrapolation {
            before: ExtrapolationMode::Hold,
            after: ExtrapolationMode::Hold,
        },
    });
    let before = title.clone();
    let mut input = store.edit();
    values_mut(&mut input).text.content = "新的文字\nEdited".into();
    let output = plan(&store, &input).unwrap();
    let after = element(&output);
    assert_eq!(after.visual, before.visual);
    assert_eq!(after.channels, before.channels);
    assert_eq!(after.source_span, before.source_span);
    assert_eq!(after.timeline_range, before.timeline_range);
    assert_eq!(
        output.selection,
        Some(TimelineTarget {
            track_id: "titles".into(),
            element_id: "title".into()
        })
    );
}

#[test]
fn unchanged_edit_returns_no_empty_apply_even_with_valid_authority() {
    let store = Store::with_title();
    let mut input = store.edit();
    values_mut(&mut input).duration = TextTimeInput::Exact {
        time: time(100, 20),
    };
    let output = plan(&store, &input).unwrap();
    assert!(output.transaction.is_none());
    assert!(output.initial.is_some());
    assert_eq!(store.authorizations.get(), 2);
    assert_eq!(store.reads.get(), 2);
}

#[test]
fn track_scope_lock_and_missing_font_are_rechecked_by_authoritative_model() {
    let mut store = Store::with_title();
    let mut input = store.edit();
    values_mut(&mut input).text.content = "Changed".into();
    store.snapshot.project.sequences[0].tracks[1].locked = true;
    assert_eq!(
        plan(&store, &input).unwrap_err().code,
        "CONSTRAINT_VIOLATION"
    );
    store.snapshot.project.sequences[0].tracks[1].locked = false;
    if let TextPlanCommand::Edit { target, .. } = &mut input.command {
        target.track_id = store.snapshot.project.sequences[0].tracks[0].id.clone();
    }
    assert!(plan(&store, &input).is_err());
    input.sequence_id = "another-scene".into();
    assert_eq!(plan(&store, &input).unwrap_err().code, "NOT_FOUND");
    input = store.edit();
    values_mut(&mut input).text.font = FontBinding::Locked {
        font_id: "missing".into(),
    };
    assert_eq!(plan(&store, &input).unwrap_err().code, "NOT_FOUND");
    values_mut(&mut input).text.font = FontBinding::Locked {
        font_id: "font".into(),
    };
    values_mut(&mut input).text.font_weight = FontWeight::Bold;
    assert!(
        plan(&store, &input)
            .unwrap_err()
            .message
            .contains("weight and style")
    );
}

#[test]
fn read_only_revocation_wrong_scope_and_both_stale_head_windows_fail_closed() {
    let mut store = Store::new();
    let input = store.create();
    store.deny_after = Some(0);
    assert_eq!(plan(&store, &input).unwrap_err().code, "NOT_AUTHORIZED");
    assert_eq!(store.reads.get(), 0);
    let mut store = Store::new();
    store.deny_after = Some(1);
    assert_eq!(plan(&store, &input).unwrap_err().code, "NOT_AUTHORIZED");
    assert_eq!(store.reads.get(), 1);
    let mut store = Store::new();
    store.snapshot.revision = "already-advanced".into();
    assert_eq!(plan(&store, &input).unwrap_err().code, "REVISION_CONFLICT");
    let mut store = Store::new();
    store.advance_after_read = true;
    assert_eq!(plan(&store, &input).unwrap_err().code, "REVISION_CONFLICT");
    assert_eq!(store.reads.get(), 2);
    let mut store = Store::new();
    store.wrong_scope = true;
    assert_eq!(plan(&store, &input).unwrap_err().code, "STORAGE_ERROR");
}

#[test]
fn exact_bold_italic_face_and_combined_style_survive_public_plan_and_later_content_edit() {
    let mut store = Store::with_title();
    let mut face = store.snapshot.project.fonts[0].clone();
    face.id = "bold-italic-face".into();
    face.weight = 700;
    face.style = FontStyle::Italic;
    face.content = AssetVersion {
        id: format!("av_{}", "b".repeat(64)),
        content_hash: "b".repeat(64),
        size: 200,
    };
    store.snapshot.project.fonts.push(face.clone());
    let before = store.snapshot.project.sequences[0].tracks[1].elements[0].clone();
    let mut input = store.edit();
    let text = &mut values_mut(&mut input).text;
    text.font = FontBinding::Locked {
        font_id: face.id.clone(),
    };
    text.font_weight = FontWeight::Bold;
    text.font_style = FontStyle::Italic;
    text.decoration = TextDecoration::LineThrough;
    text.letter_spacing_px = -1.25;
    text.line_height_multiplier = 1.6;
    text.background.enabled = true;
    text.background.corner_radius_percent = 37.5;
    text.background.padding_x_at_size15 = 7.25;
    text.background.padding_y_at_size15 = 11.5;
    text.background.offset_x_px = -13.75;
    text.background.offset_y_px = 9.5;
    text.background.color.alpha = 0.45;
    let expected = text.clone();
    let output = plan(&store, &input).unwrap();
    let tx = output.transaction.as_ref().unwrap();
    assert_eq!(tx.idempotency_key, input.idempotency_key);
    assert_eq!(tx.operations.len(), 1);
    let proposed = plan_public(&store, tx).unwrap();
    assert_eq!(
        proposed.fonts,
        crate::scene::public::project(&store.snapshot).project.fonts
    );
    let title = &proposed.sequences[0].tracks[1].elements[0];
    assert_eq!(
        title.content,
        ElementContent::Text {
            text: expected.clone()
        }
    );
    assert_eq!(title.visual, before.visual);
    assert_eq!(title.source_span, before.source_span);
    assert_eq!(title.channels, before.channels);
    assert_eq!(title.timeline_range, before.timeline_range);
    // Simulate the next authorized fixed head, without pretending this pure
    // fixture exercises the SQLite history journal or native glyph coverage.
    store.snapshot.project.sequences[0].tracks[1].elements[0] = title.clone();
    let mut later = store.edit();
    values_mut(&mut later).text.content = "后续文字\nLater title".into();
    let next = plan(&store, &later).unwrap();
    let ElementContent::Text { text } = &element(&next).content else {
        panic!()
    };
    let mut retained = expected;
    retained.content = "后续文字\nLater title".into();
    assert_eq!(*text, retained);
}
