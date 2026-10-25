use super::*;
use opencut_contracts::{media_import::*, project_history::*, source_audio::*};

fn head(app: &EditorApplication) -> EditorSnapshot {
    app.inspect_scene_project(&InspectProject {
        project_id: "p".into(),
        revision: None,
    })
    .unwrap()
}
fn enabled(snapshot: &EditorSnapshot) -> bool {
    let ElementContent::Video {
        source_audio_enabled,
        ..
    } = snapshot.project.sequences[0].tracks[0].elements[0].content
    else {
        panic!("source video")
    };
    source_audio_enabled
}
fn request(
    snapshot: &EditorSnapshot,
    key: &str,
    command: SourceAudioCommand,
) -> SourceAudioRequest {
    SourceAudioRequest {
        project_id: "p".into(),
        expected_revision: snapshot.revision.clone(),
        sequence_id: "main".into(),
        track_id: snapshot.project.sequences[0].main_track_id.clone(),
        element_id: "video".into(),
        idempotency_key: key.into(),
        label: key.into(),
        command,
    }
}

#[test]
fn source_audio_public_transaction_is_atomic_replayable_and_restoration_keeps_detached_audio() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("workspace.sqlite");
    let mut app = EditorApplication::open(&database).unwrap();
    let initial = create(&mut app, "p");
    let (version, locator) = media(&app, directory.path(), "p");
    // A synthetic native observation isolates editing semantics. These tiny
    // CAS bytes are not claimed to be decodable video/audio.
    let raw: String = app
        .connection
        .query_row(
            "SELECT asset_json FROM project_media WHERE media_id='media-fixture'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let mut observation: Asset = serde_json::from_str(&raw).unwrap();
    observation.kind = AssetKind::Video;
    observation.duration = Some(opencut_contracts::Time {
        ticks: "4".into(),
        timebase: 1,
    });
    observation.metadata.insert("has_audio".into(), json!(true));
    app.connection
        .execute(
            "UPDATE project_media SET asset_json=?1 WHERE media_id='media-fixture'",
            [serde_json::to_string(&observation).unwrap()],
        )
        .unwrap();
    let binding = MediaImportRequest {
        project_id: "p".into(),
        expected_revision: initial.revision.clone(),
        sequence_id: "main".into(),
        idempotency_key: "seed-video".into(),
        label: "seed-video".into(),
        ripple: false,
        bindings: vec![MediaBinding {
            media_id: "media-fixture".into(),
            version,
            asset_id: "source".into(),
            name: "Source".into(),
            mode: MediaBindingMode::Bind,
        }],
        placements: vec![MediaPlacement {
            element_id: "video".into(),
            name: "Video".into(),
            track_id: initial.project.sequences[0].main_track_id.clone(),
            asset_id: "source".into(),
            start: opencut_contracts::Time {
                ticks: "0".into(),
                timebase: 1,
            },
            content: MediaPlacementContent::Video {
                timing: MediaTiming::SourceRange {
                    range: opencut_contracts::TimeRange {
                        start: opencut_contracts::Time {
                            ticks: "1".into(),
                            timebase: 1,
                        },
                        duration: opencut_contracts::Time {
                            ticks: "2".into(),
                            timebase: 1,
                        },
                    },
                    rate: RationalRate {
                        numerator: 1,
                        denominator: 1,
                    },
                    maintain_pitch: false,
                },
                source_audio_enabled: true,
            },
        }],
        new_tracks: vec![],
    };
    let import = app.plan_scene_media_import(&binding).unwrap();
    app.apply_editor_edit(&import.transaction).unwrap();
    app.set_principal_policy(PrincipalPolicy {
        principal_id: "editor".into(),
        expected_revision: "0".into(),
        enabled: true,
        grants: vec![ProjectGrant {
            project_id: "p".into(),
            actions: vec![
                ProjectAction::Read,
                ProjectAction::Edit,
                ProjectAction::Restore,
            ],
        }],
    })
    .unwrap();
    let issued = app.issue_credential("editor").unwrap();
    let caller = app.authenticate(&issued.secret).unwrap();
    app.set_caller(caller.clone());
    let before = head(&app);
    let intent = request(
        &before,
        "detach",
        SourceAudioCommand::Separate {
            audio_element_id: "detached".into(),
            destination: SourceAudioDestination::FirstAvailable {
                fallback_track_id: "audio".into(),
            },
        },
    );
    let before_counts = counts(&app);
    let plan = app.plan_scene_source_audio(&intent).unwrap();
    assert_eq!(counts(&app), before_counts);
    assert!(!serde_json::to_string(&plan).unwrap().contains(&locator));
    assert_eq!(app.plan_scene_source_audio(&intent).unwrap(), plan);
    app.connection.execute_batch("CREATE TRIGGER fail_source_audio BEFORE INSERT ON audit_records BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    assert!(app.apply_editor_edit(&plan.transaction).is_err());
    assert_eq!(head(&app), before);
    assert_eq!(counts(&app), before_counts);
    app.connection
        .execute_batch("DROP TRIGGER fail_source_audio")
        .unwrap();
    let detached = app.apply_editor_edit(&plan.transaction).unwrap();
    let detached_counts = counts(&app);
    assert_eq!(app.apply_editor_edit(&plan.transaction).unwrap(), detached);
    assert_eq!(counts(&app), detached_counts);
    assert_eq!(
        app.plan_scene_source_audio(&intent).unwrap_err().code,
        "REVISION_CONFLICT"
    );
    drop(app);
    let mut app = EditorApplication::open(&database).unwrap();
    app.set_caller(caller.clone());
    let separated = head(&app);
    assert_eq!(separated.revision, detached.revision);
    assert!(!enabled(&separated));
    let audio = separated.project.sequences[0]
        .tracks
        .iter()
        .find(|t| t.id == "audio")
        .unwrap()
        .elements[0]
        .clone();
    assert_eq!(audio.id, "detached");
    assert_eq!(
        audio.source_span,
        before.project.sequences[0].tracks[0].elements[0].source_span
    );
    assert_eq!(
        audio.timeline_range,
        before.project.sequences[0].tracks[0].elements[0].timeline_range
    );
    let restore = app
        .plan_scene_source_audio(&request(&separated, "restore", SourceAudioCommand::Restore))
        .unwrap();
    // Current edit authority is still required after read-only planning.
    app.set_caller(Caller::LocalOwner);
    app.set_principal_policy(PrincipalPolicy {
        principal_id: "editor".into(),
        expected_revision: "1".into(),
        enabled: true,
        grants: vec![ProjectGrant {
            project_id: "p".into(),
            actions: vec![ProjectAction::Read],
        }],
    })
    .unwrap();
    app.set_caller(caller.clone());
    assert_eq!(
        app.apply_editor_edit(&restore.transaction)
            .unwrap_err()
            .code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(head(&app), separated);
    app.set_caller(Caller::LocalOwner);
    app.set_principal_policy(PrincipalPolicy {
        principal_id: "editor".into(),
        expected_revision: "2".into(),
        enabled: true,
        grants: vec![ProjectGrant {
            project_id: "p".into(),
            actions: vec![
                ProjectAction::Read,
                ProjectAction::Edit,
                ProjectAction::Restore,
            ],
        }],
    })
    .unwrap();
    app.set_caller(caller);
    let restored = app.apply_editor_edit(&restore.transaction).unwrap();
    assert!(enabled(&head(&app)));
    assert_eq!(head(&app).project.sequences[0].tracks[1].elements[0], audio);
    app.apply_project_undo(&UndoRequest {
        project_id: "p".into(),
        expected_revision: restored.revision,
        target_transaction_id: restored.transaction_id,
        idempotency_key: "undo-restore".into(),
    })
    .unwrap();
    assert!(!enabled(&head(&app)));
    assert_eq!(head(&app).project.sequences[0].tracks[1].elements[0], audio);
}
