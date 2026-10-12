use super::*;
use opencut_contracts::{AssetVersion, FrameRate, Time, scene_bookmarks::SceneBookmarkV1};
fn document() -> ProjectDocumentV4 {
    new_document(&CreateProject {
        id: "p".into(),
        name: "Original".into(),
        sequence_id: "main".into(),
        width: 64,
        height: 48,
        fps: FrameRate {
            numerator: 25,
            denominator: 1,
        },
    })
    .unwrap()
}
fn bookmark(id: &str) -> SceneBookmarkV1 {
    SceneBookmarkV1 {
        id: id.into(),
        sequence_id: "main".into(),
        time: Time {
            ticks: "1".into(),
            timebase: 1,
        },
        duration: None,
        color: None,
        note: None,
    }
}
fn planned(
    from: &ProjectDocumentV4,
    to: &ProjectDocumentV4,
    head: &ProjectDocumentV4,
) -> PlannedHistoryV4 {
    merge::plan(
        merge::Context {
            direction: HistoryDirectionV4::Undo,
            revision: "fixed",
            original: "target",
            undo: None,
        },
        from,
        to,
        head,
    )
}
#[test]
fn preserves_independent_bookmarks_fonts_channels_and_global_settings() {
    let before = document();
    let mut after = before.clone();
    after.name = "Target name".into();
    let mut head = after.clone();
    head.sequences[0].bookmarks.push(bookmark("later"));
    head.fonts.push(FontResourceV4 {
        id: "font".into(),
        family: "Exact face".into(),
        weight: 400,
        style: FontStyleV4::Normal,
        face_index: 0,
        content: AssetVersion {
            id: format!("av_{}", "a".repeat(64)),
            content_hash: "a".repeat(64),
            size: 7,
        },
        origin: ResourceOriginV4::LocalImport {
            original_name: "face.ttf".into(),
        },
        license: None,
    });
    let track_id = head.sequences[0].main_track_id.clone();
    head.sequences[0].tracks[0]
        .channels
        .push(AnimationChannelV4::Scalar {
            id: "channel".into(),
            target: ChannelTargetV4::Track { track_id },
            binding: ScalarBindingV4::Property {
                parameter: ScalarParameterV4::Opacity,
            },
            extrapolation: ExtrapolationV4 {
                before: ExtrapolationModeV4::Hold,
                after: ExtrapolationModeV4::Hold,
            },
            keys: vec![ScalarKeyV4 {
                id: "key".into(),
                segment_id: None,
                time: Time {
                    ticks: "0".into(),
                    timebase: 1,
                },
                value: 0.5,
                left_handle: None,
                right_handle: None,
                segment_to_next: ScalarSegmentV4::Linear,
                tangent_mode: TangentModeV4::Auto,
            }],
        });
    if let SettingsProfileV4::ClassicGlobal { settings } = &mut head.settings {
        settings.canvas_size.width = 96;
    }
    validate_project_v4(&head).unwrap();
    let result = planned(&after, &before, &head);
    assert!(result.plan.conflicts.is_empty());
    let mut expected = head.clone();
    expected.name = before.name;
    assert_eq!(result.proposed_project.unwrap(), expected);
    let public = serde_json::to_value(&result.plan).unwrap();
    assert_eq!(public["changes"][0]["path"], "/name");
    assert!(public.get("proposed_project").is_none());
    assert!(public["changes"][0].get("before").is_none());
}
#[test]
fn stable_bookmark_ids_allow_independent_additions_and_detect_dependencies() {
    let before = document();
    let mut after = before.clone();
    after.sequences[0].bookmarks.push(bookmark("target"));
    let mut head = after.clone();
    head.sequences[0].bookmarks.push(bookmark("later"));
    let result = planned(&after, &before, &head);
    assert!(result.plan.conflicts.is_empty());
    assert_eq!(
        result.proposed_project.unwrap().sequences[0].bookmarks,
        vec![bookmark("later")]
    );
    head.sequences[0].bookmarks[0].note = Some("Dependent note".into());
    let conflict = planned(&after, &before, &head);
    assert!(conflict.proposed_project.is_none());
    assert_eq!(
        conflict.plan.conflicts[0].path,
        "/sequences/main/bookmarks/target"
    );
    assert_eq!(
        conflict.plan.conflicts[0].reason,
        HistoryConflictReasonV4::DependentChange
    );
}
#[test]
fn locked_content_cannot_be_inverted_but_lock_action_itself_can() {
    let before = document();
    let mut after = before.clone();
    after.sequences[0].tracks[0].name = "Target track".into();
    let mut head = after.clone();
    head.sequences[0].tracks[0].locked = true;
    let conflict = planned(&after, &before, &head);
    assert_eq!(
        conflict.plan.conflicts[0].reason,
        HistoryConflictReasonV4::LockedTrack
    );
    let unlock = planned(&head, &after, &head);
    assert!(unlock.plan.conflicts.is_empty());
    assert!(!unlock.proposed_project.unwrap().sequences[0].tracks[0].locked);
}
#[test]
fn undo_inserted_tracks_preserves_locked_track_and_independent_edit() {
    let mut before = document();
    let mut protected = before.sequences[0].tracks[0].clone();
    protected.id = "protected".into();
    protected.locked = true;
    before.sequences[0].tracks.push(protected.clone());
    let mut after = before.clone();
    for id in ["pasted-first", "pasted-second"] {
        let mut added = before.sequences[0].tracks[0].clone();
        added.id = id.into();
        after.sequences[0].tracks.insert(1, added);
    }
    let mut head = after.clone();
    head.sequences[0].bookmarks.push(bookmark("independent"));
    let undo = planned(&after, &before, &head);
    assert!(undo.plan.conflicts.is_empty());
    let mut expected = before.clone();
    expected.sequences[0]
        .bookmarks
        .push(bookmark("independent"));
    let reverted = undo.proposed_project.unwrap();
    assert_eq!(reverted, expected);
    assert_eq!(reverted.sequences[0].tracks[1], protected);
    let redo = merge::plan(
        merge::Context {
            direction: HistoryDirectionV4::Redo,
            revision: "after-undo",
            original: "target",
            undo: Some("undo-transaction"),
        },
        &before,
        &after,
        &reverted,
    );
    assert!(redo.plan.conflicts.is_empty());
    assert_eq!(redo.proposed_project.unwrap(), head);
}

#[test]
fn same_property_and_invalid_dependencies_produce_digest_only_conflicts() {
    let before = document();
    let mut after = before.clone();
    after.name = "Target".into();
    let mut head = after.clone();
    head.name = "Later name".into();
    let conflict = planned(&after, &before, &head);
    assert_eq!(conflict.plan.conflicts[0].path, "/name");
    assert_eq!(
        conflict.plan.conflicts[0]
            .expected_sha256
            .as_ref()
            .unwrap()
            .len(),
        64
    );
    assert_eq!(
        conflict.plan.conflicts[0]
            .actual_sha256
            .as_ref()
            .unwrap()
            .len(),
        64
    );
    let mut after = before.clone();
    let mut new = after.sequences[0].tracks[0].clone();
    new.id = "new-main".into();
    after.sequences[0].tracks.push(new);
    let mut head = after.clone();
    head.sequences[0].main_track_id = "new-main".into();
    let invalid = planned(&after, &before, &head);
    assert!(invalid.proposed_project.is_none());
    assert_eq!(
        invalid.plan.conflicts[0].reason,
        HistoryConflictReasonV4::ProjectConstraint
    );
}
