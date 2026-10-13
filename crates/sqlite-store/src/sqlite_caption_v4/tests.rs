use super::*;
use opencut_contracts::{
    Asset, AssetKind, CreateProject, FrameRate, project_v4::*, project_v4_history::*,
    project_v4_ops::*,
};
fn setup() -> (EditorApplication, tempfile::TempDir, SubtitleImportV4) {
    let directory = tempfile::tempdir().unwrap();
    let mut app = EditorApplication::open(":memory:").unwrap();
    let initial = app
        .create_v4_project(&CreateV4Project {
            project: CreateProject {
                id: "p".into(),
                name: "p".into(),
                sequence_id: "s".into(),
                width: 320,
                height: 180,
                fps: FrameRate {
                    numerator: 30,
                    denominator: 1,
                },
            },
            idempotency_key: "create".into(),
        })
        .unwrap();
    // Opaque CAS bytes are sufficient for persistence tests; this is not font
    // parsing, rasterization or real media acceptance.
    let file = directory.path().join("font-test.bin");
    std::fs::write(&file, b"synthetic persistent font resource").unwrap();
    let version = opencut_asset_store::AssetStore::open(app.asset_store_root())
        .unwrap()
        .pin_asset(&Asset {
            id: "font-source".into(),
            name: "font".into(),
            kind: AssetKind::Graphic,
            uri: file.to_string_lossy().into(),
            duration: None,
            version: None,
            metadata: Default::default(),
        })
        .unwrap()
        .version
        .unwrap();
    let font = FontResourceV4 {
        id: "font".into(),
        family: "Test".into(),
        weight: 400,
        style: FontStyleV4::Normal,
        face_index: 0,
        content: version,
        origin: ResourceOriginV4::LocalImport {
            original_name: "font-test.bin".into(),
        },
        license: None,
    };
    let track = TrackV4 {
        id: "captions".into(),
        name: "Captions".into(),
        kind: TrackKindV4::Caption,
        enabled: true,
        muted: false,
        locked: false,
        opacity: 1.0,
        gain: GainV4::Decibels { value: 0.0 },
        elements: vec![],
        channels: vec![],
    };
    let bound = app
        .apply_v4_project(&ApplyV4Project {
            project_id: "p".into(),
            expected_revision: initial.revision,
            idempotency_key: "bind".into(),
            label: "Caption resources".into(),
            operations: vec![
                ProjectOperationV4::FontPut { font },
                ProjectOperationV4::TrackPut {
                    sequence_id: "s".into(),
                    track,
                },
            ],
        })
        .unwrap();
    let style = CaptionStyleV4 {
        font: FontBindingV4::Locked {
            font_id: "font".into(),
        },
        font_size: FontSizeV4::CanvasPixels { value: 24.0 },
        color: ColorV4 {
            encoding: ColorEncodingV4::Srgb,
            red: 1.0,
            green: 1.0,
            blue: 1.0,
            alpha: 1.0,
        },
        background_color: None,
        max_width_px: None,
        line_height_px: None,
        text_align: TextAlignV4::Center,
        layout: CaptionLayoutProfileV4::PinnedCaptionV4,
    };
    (
        app,
        directory,
        SubtitleImportV4 {
            project_id: "p".into(),
            expected_revision: bound.revision,
            sequence_id: "s".into(),
            track_id: "captions".into(),
            idempotency_key: "subtitles".into(),
            format: SubtitleFormatV4::Srt,
            content: "1\n00:00:00,100 --> 00:00:01,250\nHello 字幕\n\n".into(),
            style,
        },
    )
}
fn head(app: &EditorApplication) -> ProjectSnapshotV4 {
    app.inspect_v4_project(&InspectV4Project {
        project_id: "p".into(),
        revision: None,
    })
    .unwrap()
}
#[test]
fn caption_import_is_one_original_receipt_and_ordinary_atomic_undo_redo() {
    let (mut app, _dir, input) = setup();
    let receipt = app.import_subtitles_v4(&input).unwrap();
    assert_eq!(receipt.cue_count, 1);
    assert_eq!(head(&app).project.sequences[0].captions.len(), 1);
    let replay = app.import_subtitles_v4(&input).unwrap();
    assert_eq!(replay, receipt);
    let mut changed = input.clone();
    changed.content = changed.content.replace("Hello", "Different");
    assert_eq!(
        app.import_subtitles_v4(&changed).unwrap_err().code,
        "IDEMPOTENCY_KEY_REUSED"
    );
    let undo = app
        .apply_v4_undo(&UndoV4Project {
            project_id: "p".into(),
            expected_revision: receipt.transaction.revision.clone(),
            target_transaction_id: receipt.transaction.transaction_id.clone(),
            idempotency_key: "undo".into(),
        })
        .unwrap();
    assert!(head(&app).project.sequences[0].captions.is_empty());
    app.apply_v4_redo(&RedoV4Project {
        project_id: "p".into(),
        expected_revision: undo.transaction.revision,
        undo_transaction_id: undo.transaction.transaction_id,
        idempotency_key: "redo".into(),
    })
    .unwrap();
    assert_eq!(head(&app).project.sequences[0].captions.len(), 1);
}
#[test]
fn invalid_import_and_locked_track_leave_revision_and_journal_unchanged() {
    let (mut app, _dir, mut input) = setup();
    let before = head(&app);
    input.content = "1\n00:00:02,000 --> 00:00:01,000\nBad\n".into();
    assert!(app.import_subtitles_v4(&input).is_err());
    assert_eq!(head(&app), before);
    let locked = app
        .apply_v4_project(&ApplyV4Project {
            project_id: "p".into(),
            expected_revision: before.revision,
            idempotency_key: "lock".into(),
            label: "Lock caption track".into(),
            operations: vec![ProjectOperationV4::TrackLockSet {
                sequence_id: "s".into(),
                track_id: "captions".into(),
                locked: true,
            }],
        })
        .unwrap();
    input.expected_revision = locked.revision.clone();
    input.content = "1\n00:00:00,100 --> 00:00:01,250\nHello\n\n".into();
    assert_eq!(
        app.import_subtitles_v4(&input).unwrap_err().code,
        "TRACK_LOCKED"
    );
    assert_eq!(head(&app).revision, locked.revision);
}

fn caption_stage(
    input: &SubtitleImportV4,
) -> opencut_contracts::edit_recovery_v3::StageEditRecoveryV3 {
    use opencut_contracts::edit_recovery_v3::*;
    StageEditRecoveryV3 {
        context: EditRecoveryContextV3 {
            intent: EditRecoveryIntentV3::SubtitleImportV4 {
                request: input.clone(),
            },
            sequence_id: Some(input.sequence_id.clone()),
        },
    }
}
#[test]
fn caption_recovery_keeps_exact_original_receipt_until_explicit_ack() {
    use opencut_contracts::edit_recovery_v3::*;
    let (mut app, _dir, input) = setup();
    let before = head(&app);
    let staged = app.stage_edit_recovery_v3(&caption_stage(&input)).unwrap();
    assert!(staged.receipt.is_none());
    assert_eq!(head(&app), before);
    let ack = AckEditRecoveryV3 {
        project_id: "p".into(),
        recovery_id: staged.recovery_id.clone(),
    };
    assert_eq!(
        app.ack_edit_recovery_v3(&ack).unwrap_err().code,
        "RECOVERY_UNRESOLVED"
    );
    let committed = app.import_subtitles_v4(&input).unwrap();
    let read = ReadEditRecoveryV3 {
        project_id: "p".into(),
        recovery_id: Some(staged.recovery_id),
    };
    let restored = app.inspect_edit_recovery_v3(&read).unwrap().record.unwrap();
    assert_eq!(restored.context, staged.context);
    assert_eq!(restored.receipt, Some(committed.clone()));
    let resolved = app.ack_edit_recovery_v3(&ack).unwrap();
    assert_eq!(resolved.resolution, EditRecoveryResolutionV3::Saved);
    assert_eq!(resolved.receipt, Some(committed));
    assert!(
        app.inspect_edit_recovery_v3(&read)
            .unwrap()
            .record
            .is_none()
    );
}
#[test]
fn caption_recovery_rejects_changed_key_payload_and_other_protocol_slots() {
    use opencut_contracts::{
        edit_recovery_v2::*, font_import_v1::ImportFontV1, font_recovery_v1::*,
    };
    let (mut app, _dir, input) = setup();
    let staged = app.stage_edit_recovery_v3(&caption_stage(&input)).unwrap();
    let mut changed = input.clone();
    changed.content = changed.content.replace("Hello", "Changed");
    assert_eq!(
        app.stage_edit_recovery_v3(&caption_stage(&changed))
            .unwrap_err()
            .code,
        "IDEMPOTENCY_KEY_REUSED"
    );
    changed.idempotency_key = "different-key".into();
    assert_eq!(
        app.stage_edit_recovery_v3(&caption_stage(&changed))
            .unwrap_err()
            .code,
        "RECOVERY_CONFLICT"
    );
    let v2 = StageEditRecoveryV2 {
        context: EditRecoveryContextV2 {
            intent: EditRecoveryIntentV2::Restore {
                request: RestoreV4Revision {
                    project_id: "p".into(),
                    expected_revision: input.expected_revision.clone(),
                    target_revision: input.expected_revision.clone(),
                    asset_versions: Default::default(),
                    idempotency_key: "restore".into(),
                },
            },
            sequence_id: Some("s".into()),
        },
    };
    assert_eq!(
        app.stage_edit_recovery_v2(&v2).unwrap_err().code,
        "RECOVERY_CONFLICT"
    );
    let font = StageFontRecoveryV1 {
        context: FontRecoveryContextV1 {
            request: ImportFontV1 {
                project_id: "p".into(),
                expected_revision: input.expected_revision.clone(),
                idempotency_key: "font-import".into(),
                catalog_id: "google-lato-regular-v1".into(),
                font_id: "other-font".into(),
                required_text: "AV To".into(),
            },
            sequence_id: Some("s".into()),
        },
    };
    assert_eq!(
        app.stage_font_recovery_v1(&font).unwrap_err().code,
        "RECOVERY_CONFLICT"
    );
    assert_eq!(
        app.stage_edit_recovery_v3(&caption_stage(&input)).unwrap(),
        staged
    );
}
#[test]
fn caption_recovery_revocation_retains_slot_without_disclosure_retry_or_ack() {
    use opencut_api_contracts::access::ProjectAction;
    use opencut_contracts::edit_recovery_v3::*;
    use opencut_editor_application::access::{Caller, PrincipalPolicy, ProjectGrant};
    let (mut app, _dir, input) = setup();
    let policy = |revision: &str, actions| PrincipalPolicy {
        principal_id: "caption-editor".into(),
        expected_revision: revision.into(),
        enabled: true,
        grants: vec![ProjectGrant {
            project_id: "p".into(),
            actions,
        }],
    };
    app.set_principal_policy(policy("0", vec![ProjectAction::Read, ProjectAction::Edit]))
        .unwrap();
    let credential = app.issue_credential("caption-editor").unwrap();
    let caller = app.authenticate(&credential.secret).unwrap();
    app.set_caller(caller.clone());
    let staged = app.stage_edit_recovery_v3(&caption_stage(&input)).unwrap();
    let read = ReadEditRecoveryV3 {
        project_id: "p".into(),
        recovery_id: Some(staged.recovery_id.clone()),
    };
    let ack = AckEditRecoveryV3 {
        project_id: "p".into(),
        recovery_id: staged.recovery_id,
    };
    app.set_caller(Caller::LocalOwner);
    app.set_principal_policy(policy("1", vec![ProjectAction::Read]))
        .unwrap();
    app.set_caller(caller.clone());
    assert_eq!(
        app.inspect_edit_recovery_v3(&read).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(
        app.stage_edit_recovery_v3(&caption_stage(&input))
            .unwrap_err()
            .code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(
        app.import_subtitles_v4(&input).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(
        app.ack_edit_recovery_v3(&ack).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
    app.set_caller(Caller::LocalOwner);
    app.revoke_credential(&credential.credential_id).unwrap();
    app.set_caller(caller);
    assert_eq!(
        app.inspect_edit_recovery_v3(&read).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(
        app.connection
            .query_row("SELECT count(*) FROM edit_recovery_v2", [], |r| r
                .get::<_, u32>(0))
            .unwrap(),
        1
    );
}
