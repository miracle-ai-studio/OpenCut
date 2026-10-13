use super::*;
use opencut_contracts::font_recovery_v1::*;
use opencut_contracts::{font_import_v1::ImportFontV1, project_v4_history::*};
use opencut_editor_application::{
    access::{Caller, PrincipalPolicy, ProjectGrant},
    font_import_v1::{catalog::*, *},
};
use std::{cell::Cell, path::PathBuf, rc::Rc};

fn fixture(app: &mut EditorApplication) -> (ProjectSnapshotV4, ImportFontV1) {
    let source = app
        .create_v4_project(&CreateV4Project {
            project: CreateProject {
                id: "font-project".into(),
                name: "Fonts".into(),
                sequence_id: "main".into(),
                width: 320,
                height: 180,
                fps: FrameRate {
                    numerator: 24,
                    denominator: 1,
                },
            },
            idempotency_key: "create-font-project".into(),
        })
        .unwrap();
    let request = ImportFontV1 {
        project_id: source.project.id.clone(),
        expected_revision: source.revision.clone(),
        idempotency_key: "font-import".into(),
        catalog_id: "google-lato-regular-v1".into(),
        font_id: "locked-font".into(),
        required_text: "AV To".into(),
    };
    (source, request)
}
fn scoped_caller(app: &mut EditorApplication, project: &str) -> (Caller, String) {
    app.set_principal_policy(PrincipalPolicy {
        principal_id: "font-agent".into(),
        expected_revision: "0".into(),
        enabled: true,
        grants: vec![ProjectGrant {
            project_id: project.into(),
            actions: vec![
                ProjectAction::Read,
                ProjectAction::Edit,
                ProjectAction::Restore,
            ],
        }],
    })
    .unwrap();
    let issued = app.issue_credential("font-agent").unwrap();
    (
        app.authenticate(&issued.secret).unwrap(),
        issued.credential_id,
    )
}

#[test]
fn font_import_resource_authority_admits_only_its_exact_pinned_font() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let (source, request) = fixture(&mut app);
    let (caller, _) = scoped_caller(&mut app, &request.project_id);
    app.set_caller(caller);
    app.authorize_v4(V4Intent::FontImport(&request)).unwrap();
    let mut proposed = source.project.clone();
    proposed
        .fonts
        .push(expected_import_resource_v1(&request).unwrap());
    let authorized = |document: &ProjectDocumentV4| {
        authorize_document(
            &app.connection,
            &app.connection.1,
            V4Intent::FontImport(&request),
            document,
        )
    };
    authorized(&proposed).unwrap();
    for change in 0..5 {
        let mut forged = proposed.clone();
        match change {
            0 => forged.fonts[0].content.content_hash = "a".repeat(64),
            1 => forged.fonts[0].family = "Other family".into(),
            2 => forged.fonts[0].license.as_mut().unwrap().notice = Some("stripped notice".into()),
            3 => forged.fonts[0].id = "another-font".into(),
            _ => {
                let mut extra = forged.fonts[0].clone();
                extra.id = "extra".into();
                forged.fonts.push(extra);
            }
        }
        assert_eq!(authorized(&forged).unwrap_err().code, "NOT_AUTHORIZED");
    }
    let legacy_apply = ApplyV4Project {
        project_id: request.project_id.clone(),
        expected_revision: request.expected_revision.clone(),
        idempotency_key: "raw-font".into(),
        label: "Raw font".into(),
        operations: vec![ProjectOperationV4::FontPut {
            font: proposed.fonts[0].clone(),
        }],
    };
    assert_eq!(
        app.authorize_v4(V4Intent::Apply(&legacy_apply))
            .unwrap_err()
            .code,
        "NOT_AUTHORIZED"
    );
}

struct FixtureSource {
    bytes: Vec<u8>,
    calls: Cell<usize>,
}
impl FontSourceFetchV1 for FixtureSource {
    fn fetch(
        &self,
        _: &str,
        _: &FontFetchPolicyV1,
        checkpoint: &mut dyn FontImportCheckpointV1,
    ) -> Result<Vec<u8>, EditorError> {
        checkpoint.check()?;
        self.calls.set(self.calls.get() + 1);
        Ok(self.bytes.clone())
    }
}
// This fixture isolates durable publication; native parsing of these same
// independently pinned bytes is tested by the native-services fixture test.
struct FixtureInspector;
impl FontInspectorV1 for FixtureInspector {
    fn inspect(
        &self,
        _: &[u8],
        _: &str,
        _: &FontImportPolicyV1,
        _: &mut dyn FontImportCheckpointV1,
    ) -> Result<ParsedFontFaceV1, EditorError> {
        Ok(ParsedFontFaceV1 {
            family: "Lato".into(),
            weight: 400,
            style: FontStyleV4::Normal,
            glyph_count: 3023,
            checked_codepoints: 5,
        })
    }
}
struct FixturePublisher {
    root: PathBuf,
    source: PathBuf,
    live: Rc<Cell<bool>>,
}
struct Published {
    _store: AssetStore,
    live: Rc<Cell<bool>>,
}
impl Drop for Published {
    fn drop(&mut self) {
        self.live.set(false);
    }
}
impl FontCasPublishV1 for FixturePublisher {
    type Lease = Published;
    fn publish(
        &self,
        font: &PreparedFontV1,
        bytes: &[u8],
        checkpoint: &mut dyn FontImportCheckpointV1,
    ) -> Result<Published, EditorError> {
        checkpoint.check()?;
        assert_eq!(std::fs::read(&self.source).unwrap(), bytes);
        let store = AssetStore::open_managed(&self.root)?;
        let pinned = store.pin_asset(&Asset {
            id: "font-bytes".into(),
            name: "Font fixture".into(),
            kind: AssetKind::Graphic,
            uri: self.source.to_str().unwrap().into(),
            duration: None,
            version: None,
            metadata: Default::default(),
        })?;
        assert_eq!(pinned.version.as_ref(), Some(&font.resource.content));
        self.live.set(true);
        Ok(Published {
            _store: store,
            live: self.live.clone(),
        })
    }
}
struct ObservedStore<'a> {
    app: &'a mut EditorApplication,
    live: Rc<Cell<bool>>,
}
impl ProjectV4Read for ObservedStore<'_> {
    fn authorize_v4(&self, intent: V4Intent<'_>) -> Result<(), EditorError> {
        self.app.authorize_v4(intent)
    }
    fn head_v4_source(&self, id: &str) -> Result<StoredProjectRevisionV4, EditorError> {
        self.app.head_v4_source(id)
    }
    fn revision_v4_source(
        &self,
        id: &str,
        rev: &str,
    ) -> Result<StoredProjectRevisionV4, EditorError> {
        self.app.revision_v4_source(id, rev)
    }
    fn replay_v4_create(
        &self,
        r: &CreateV4Project,
    ) -> Result<Option<ProjectSnapshotV4>, EditorError> {
        self.app.replay_v4_create(r)
    }
    fn replay_v4_copy(&self, r: &CopyV4Project) -> Result<Option<ProjectSnapshotV4>, EditorError> {
        self.app.replay_v4_copy(r)
    }
    fn replay_v4_edit(
        &self,
        intent: V4Intent<'_>,
    ) -> Result<Option<TransactionResult>, EditorError> {
        self.app.replay_v4_edit(intent)
    }
}
impl ProjectV4Store for ObservedStore<'_> {
    fn create_v4_atomic(
        &mut self,
        _: &CreateV4Project,
        _: PreparedV4Create,
    ) -> Result<ProjectSnapshotV4, EditorError> {
        unreachable!()
    }
    fn copy_v4_atomic(
        &mut self,
        _: &CopyV4Project,
        _: PreparedV4Create,
    ) -> Result<ProjectSnapshotV4, EditorError> {
        unreachable!()
    }
    fn commit_v4_atomic(
        &mut self,
        intent: V4Intent<'_>,
        prepared: PreparedV4Commit,
    ) -> Result<TransactionResult, EditorError> {
        assert!(
            self.live.get(),
            "CAS publication guard must survive SQL publication"
        );
        self.app.commit_v4_atomic(intent, prepared)
    }
}

#[test]
#[ignore = "requires pinned licensed public fixture in OPENCUT_FONT_FIXTURE_DIR"]
fn licensed_fixture_font_import_preserves_receipts_history_and_cas_lease() {
    let font = PathBuf::from(
        std::env::var_os("OPENCUT_FONT_FIXTURE_DIR").expect("font fixture directory"),
    )
    .join("lato-Lato-Regular.ttf");
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("workspace.sqlite");
    let mut app = EditorApplication::open(&database).unwrap();
    let (initial, request) = fixture(&mut app);
    let (caller, credential) = scoped_caller(&mut app, &request.project_id);
    app.set_caller(caller.clone());
    let recovery = app
        .stage_font_recovery_v1(&StageFontRecoveryV1 {
            context: FontRecoveryContextV1 {
                request: request.clone(),
                sequence_id: Some("main".into()),
            },
        })
        .unwrap();
    let source = FixtureSource {
        bytes: std::fs::read(&font).unwrap(),
        calls: Cell::new(0),
    };
    let live = Rc::new(Cell::new(false));
    let publisher = FixturePublisher {
        root: app.asset_store_root().into(),
        source: font,
        live: live.clone(),
    };
    let policy = FontFetchPolicyV1 {
        allow_public_font_download: true,
        ..Default::default()
    };
    let result = project_v4::import_font_v1(
        &mut ObservedStore {
            app: &mut app,
            live: live.clone(),
        },
        &source,
        &FixtureInspector,
        &publisher,
        &request,
        &policy,
        &FontImportPolicyV1::default(),
        &mut || Ok(()),
    )
    .unwrap();
    assert!(!live.get());
    assert_eq!(source.calls.get(), 1);
    let head = app
        .inspect_v4_project(&InspectV4Project {
            project_id: request.project_id.clone(),
            revision: None,
        })
        .unwrap();
    assert_eq!(
        head.project.fonts,
        vec![expected_import_resource_v1(&request).unwrap()]
    );
    let raw: String = app
        .connection
        .query_row(
            "SELECT request_json FROM editor_transactions WHERE transaction_id=?1",
            [&result.transaction_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(serde_json::from_str::<ImportFontV1>(&raw).unwrap(), request);
    let family: String = app
        .connection
        .query_row(
            "SELECT operation_family FROM idempotency WHERE scope=?1 AND idempotency_key=?2",
            params![request.project_id, request.idempotency_key],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(family, "project_v4_font_import_v1");
    let undo = app
        .apply_v4_undo(&UndoV4Project {
            project_id: request.project_id.clone(),
            expected_revision: result.revision.clone(),
            target_transaction_id: result.transaction_id.clone(),
            idempotency_key: "undo-font".into(),
        })
        .unwrap();
    let undone = app
        .inspect_v4_project(&InspectV4Project {
            project_id: request.project_id.clone(),
            revision: None,
        })
        .unwrap();
    assert_eq!(undone.project, initial.project);
    app.apply_v4_redo(&RedoV4Project {
        project_id: request.project_id.clone(),
        expected_revision: undo.transaction.revision,
        undo_transaction_id: undo.transaction.transaction_id,
        idempotency_key: "redo-font".into(),
    })
    .unwrap();
    let current = app
        .inspect_v4_project(&InspectV4Project {
            project_id: request.project_id.clone(),
            revision: None,
        })
        .unwrap();
    assert_eq!(
        app.apply_v4_project(&ApplyV4Project {
            project_id: request.project_id.clone(),
            expected_revision: current.revision.clone(),
            idempotency_key: request.idempotency_key.clone(),
            label: "Different operation".into(),
            operations: vec![ProjectOperationV4::Rename {
                name: "Must not commit".into()
            }],
        })
        .unwrap_err()
        .code,
        "IDEMPOTENCY_KEY_REUSED"
    );
    let cancelled = ImportFontV1 {
        idempotency_key: "cancelled-import".into(),
        font_id: "cancelled-font".into(),
        expected_revision: current.revision.clone(),
        ..request.clone()
    };
    let error = project_v4::import_font_v1(
        &mut app,
        &source,
        &FixtureInspector,
        &publisher,
        &cancelled,
        &policy,
        &FontImportPolicyV1::default(),
        &mut || {
            if live.get() {
                Err(EditorError::new(
                    "JOB_CANCELLED",
                    "Cancelled after native publication",
                ))
            } else {
                Ok(())
            }
        },
    )
    .unwrap_err();
    assert_eq!(error.code, "JOB_CANCELLED");
    assert!(!live.get());
    assert_eq!(
        app.inspect_v4_project(&InspectV4Project {
            project_id: request.project_id.clone(),
            revision: None
        })
        .unwrap(),
        current
    );
    let racing = ImportFontV1 {
        idempotency_key: "racing-import".into(),
        font_id: "racing-font".into(),
        expected_revision: current.revision.clone(),
        ..request.clone()
    };
    let mut advanced = false;
    let error = project_v4::import_font_v1(
        &mut app,
        &source,
        &FixtureInspector,
        &publisher,
        &racing,
        &policy,
        &FontImportPolicyV1::default(),
        &mut || {
            if live.get() && !advanced {
                let mut owner = EditorApplication::open(&database)?;
                owner.apply_v4_project(&ApplyV4Project {
                    project_id: request.project_id.clone(),
                    expected_revision: current.revision.clone(),
                    idempotency_key: "advance-during-import".into(),
                    label: "Concurrent rename".into(),
                    operations: vec![ProjectOperationV4::Rename {
                        name: "Advanced".into(),
                    }],
                })?;
                advanced = true;
            }
            Ok(())
        },
    )
    .unwrap_err();
    assert_eq!(error.code, "REVISION_CONFLICT");
    assert!(!live.get());
    assert!(advanced);
    let advanced_head = app
        .inspect_v4_project(&InspectV4Project {
            project_id: request.project_id.clone(),
            revision: None,
        })
        .unwrap();
    assert_eq!(advanced_head.project.name, "Advanced");
    assert_eq!(advanced_head.project.fonts, current.project.fonts);
    let failed_receipts: i64 = app.connection.query_row("SELECT COUNT(*) FROM idempotency WHERE scope=?1 AND idempotency_key IN ('cancelled-import','racing-import')", [&request.project_id], |row| row.get(0)).unwrap();
    assert_eq!(failed_receipts, 0);
    let acquisitions = source.calls.get();
    drop(app);
    let mut app = EditorApplication::open(&database).unwrap();
    app.set_caller(caller.clone());
    // Receipt lookup succeeds while offline, after undo/redo advanced head and
    // after the original process was closed. No acquisition or publication.
    assert_eq!(
        project_v4::import_font_v1(
            &mut app,
            &source,
            &FixtureInspector,
            &publisher,
            &request,
            &FontFetchPolicyV1::default(),
            &FontImportPolicyV1::default(),
            &mut || Ok(())
        )
        .unwrap(),
        result
    );
    assert_eq!(source.calls.get(), acquisitions);
    let read_recovery = ReadFontRecoveryV1 {
        project_id: request.project_id.clone(),
        recovery_id: Some(recovery.recovery_id.clone()),
    };
    let recovered = app
        .inspect_font_recovery_v1(&read_recovery)
        .unwrap()
        .record
        .unwrap();
    assert_eq!(recovered.context, recovery.context);
    assert_eq!(recovered.receipt, Some(result.clone()));
    // A matching idempotency row alone is insufficient: recovery also checks
    // the full original request, result and actual revision parent edge.
    let changed_journal = serde_json::to_string(&ImportFontV1 {
        required_text: "Different".into(),
        ..request.clone()
    })
    .unwrap();
    app.connection
        .execute(
            "UPDATE editor_transactions SET request_json=?1 WHERE transaction_id=?2",
            params![changed_journal, result.transaction_id],
        )
        .unwrap();
    assert_eq!(
        app.inspect_font_recovery_v1(&read_recovery)
            .unwrap_err()
            .code,
        "STORAGE_ERROR"
    );
    app.connection
        .execute(
            "UPDATE editor_transactions SET request_json=?1 WHERE transaction_id=?2",
            params![raw, result.transaction_id],
        )
        .unwrap();
    app.connection
        .execute(
            "UPDATE revisions SET parent_revision='different-parent' WHERE revision=?1",
            [&result.revision],
        )
        .unwrap();
    assert_eq!(
        app.inspect_font_recovery_v1(&read_recovery)
            .unwrap_err()
            .code,
        "STORAGE_ERROR"
    );
    app.connection
        .execute(
            "UPDATE revisions SET parent_revision=?1 WHERE revision=?2",
            params![result.base_revision, result.revision],
        )
        .unwrap();
    let saved = app
        .ack_font_recovery_v1(&AckFontRecoveryV1 {
            project_id: request.project_id.clone(),
            recovery_id: recovery.recovery_id,
        })
        .unwrap();
    assert_eq!(saved.resolution, FontRecoveryResolutionV1::Saved);
    assert_eq!(saved.receipt, Some(result.clone()));
    assert!(
        app.inspect_font_recovery_v1(&read_recovery)
            .unwrap()
            .record
            .is_none()
    );
    assert_eq!(source.calls.get(), acquisitions);
    for changed in [
        ImportFontV1 {
            required_text: "changed".into(),
            ..request.clone()
        },
        ImportFontV1 {
            catalog_id: "google-mashanzheng-regular-v1".into(),
            ..request.clone()
        },
    ] {
        assert_eq!(
            project_v4::import_font_v1(
                &mut app,
                &source,
                &FixtureInspector,
                &publisher,
                &changed,
                &policy,
                &FontImportPolicyV1::default(),
                &mut || Ok(())
            )
            .unwrap_err()
            .code,
            "IDEMPOTENCY_KEY_REUSED"
        );
    }
    app.set_caller(Caller::LocalOwner);
    app.revoke_credential(&credential).unwrap();
    app.set_caller(caller);
    assert_eq!(
        project_v4::import_font_v1(
            &mut app,
            &source,
            &FixtureInspector,
            &publisher,
            &request,
            &policy,
            &FontImportPolicyV1::default(),
            &mut || Ok(())
        )
        .unwrap_err()
        .code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(source.calls.get(), acquisitions);
}
