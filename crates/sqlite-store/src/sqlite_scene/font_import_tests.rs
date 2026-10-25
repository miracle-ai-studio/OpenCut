use super::*;
use opencut_contracts::edit_recovery::*;
use opencut_contracts::{font_import::ImportFont, project_history::*};
use opencut_editor_application::{
    access::{Caller, PrincipalPolicy, ProjectGrant},
    font_import::{catalog::*, *},
};
use std::{cell::Cell, path::PathBuf, rc::Rc};

fn fixture(app: &mut EditorApplication) -> (EditorSnapshot, ImportFont) {
    let source = app
        .create_scene_project(&CreateProjectRequest {
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
    let request = ImportFont {
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
    app.authorize_project(SceneCommitIntent::FontImport(&request))
        .unwrap();
    let mut proposed = source.project.clone();
    proposed
        .fonts
        .push(expected_import_resource(&request).unwrap());
    let authorized = |document: &EditorDocument| {
        authorize_document(
            &app.connection,
            &app.connection.1,
            SceneCommitIntent::FontImport(&request),
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
    let legacy_apply = ApplySceneProject {
        project_id: request.project_id.clone(),
        expected_revision: request.expected_revision.clone(),
        idempotency_key: "raw-font".into(),
        label: "Raw font".into(),
        operations: vec![ProjectOperation::FontPut {
            font: proposed.fonts[0].clone(),
        }],
    };
    assert_eq!(
        app.authorize_project(SceneCommitIntent::Apply(&legacy_apply))
            .unwrap_err()
            .code,
        "NOT_AUTHORIZED"
    );
}

struct FixtureSource {
    bytes: Vec<u8>,
    calls: Cell<usize>,
}
impl FontSourceFetch for FixtureSource {
    fn fetch(
        &self,
        _: &str,
        _: &FontFetchPolicy,
        checkpoint: &mut dyn FontImportCheckpoint,
    ) -> Result<Vec<u8>, EditorError> {
        checkpoint.check()?;
        self.calls.set(self.calls.get() + 1);
        Ok(self.bytes.clone())
    }
}
// This fixture isolates durable publication; native parsing of these same
// independently pinned bytes is tested by the native-services fixture test.
struct FixtureInspector;
impl FontInspector for FixtureInspector {
    fn inspect(
        &self,
        _: &[u8],
        _: &str,
        _: &FontImportPolicy,
        _: &mut dyn FontImportCheckpoint,
    ) -> Result<ParsedFontFace, EditorError> {
        Ok(ParsedFontFace {
            family: "Lato".into(),
            weight: 400,
            style: FontStyle::Normal,
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
impl FontCasPublish for FixturePublisher {
    type Lease = Published;
    fn publish(
        &self,
        font: &PreparedFont,
        bytes: &[u8],
        checkpoint: &mut dyn FontImportCheckpoint,
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
impl SceneRead for ObservedStore<'_> {
    fn authorize_project(&self, intent: SceneCommitIntent<'_>) -> Result<(), EditorError> {
        self.app.authorize_project(intent)
    }
    fn head_scene_source(&self, id: &str) -> Result<StoredProjectRevision, EditorError> {
        self.app.head_scene_source(id)
    }
    fn revision_scene_source(
        &self,
        id: &str,
        rev: &str,
    ) -> Result<StoredProjectRevision, EditorError> {
        self.app.revision_scene_source(id, rev)
    }
    fn replay_scene_create(
        &self,
        r: &CreateProjectRequest,
    ) -> Result<Option<EditorSnapshot>, EditorError> {
        self.app.replay_scene_create(r)
    }
    fn replay_scene_copy(
        &self,
        r: &CopySceneProject,
    ) -> Result<Option<EditorSnapshot>, EditorError> {
        self.app.replay_scene_copy(r)
    }
    fn replay_scene_edit(
        &self,
        intent: SceneCommitIntent<'_>,
    ) -> Result<Option<TransactionResult>, EditorError> {
        self.app.replay_scene_edit(intent)
    }
}
impl SceneStore for ObservedStore<'_> {
    fn create_scene_atomic(
        &mut self,
        _: &CreateProjectRequest,
        _: PreparedCreate,
    ) -> Result<EditorSnapshot, EditorError> {
        unreachable!()
    }
    fn copy_scene_atomic(
        &mut self,
        _: &CopySceneProject,
        _: PreparedCreate,
    ) -> Result<EditorSnapshot, EditorError> {
        unreachable!()
    }
    fn commit_scene_atomic(
        &mut self,
        intent: SceneCommitIntent<'_>,
        prepared: PreparedSceneCommit,
    ) -> Result<TransactionResult, EditorError> {
        assert!(
            self.live.get(),
            "CAS publication guard must survive SQL publication"
        );
        self.app.commit_scene_atomic(intent, prepared)
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
        .stage_edit_recovery(&StageEditRecovery {
            context: EditRecoveryContext {
                intent: EditRecoveryIntent::FontImport {
                    request: request.clone(),
                },
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
    let policy = FontFetchPolicy {
        allow_public_font_download: true,
        ..Default::default()
    };
    let result = scene::import_font_resource(
        &mut ObservedStore {
            app: &mut app,
            live: live.clone(),
        },
        &source,
        &FixtureInspector,
        &publisher,
        &request,
        &policy,
        &FontImportPolicy::default(),
        &mut || Ok(()),
    )
    .unwrap();
    assert!(!live.get());
    assert_eq!(source.calls.get(), 1);
    let head = app
        .inspect_scene_project(&InspectProject {
            project_id: request.project_id.clone(),
            revision: None,
        })
        .unwrap();
    assert_eq!(
        head.project.fonts,
        vec![expected_import_resource(&request).unwrap()]
    );
    let raw: String = app
        .connection
        .query_row(
            "SELECT request_json FROM editor_transactions WHERE transaction_id=?1",
            [&result.transaction_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(serde_json::from_str::<ImportFont>(&raw).unwrap(), request);
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
        .apply_project_undo(&UndoRequest {
            project_id: request.project_id.clone(),
            expected_revision: result.revision.clone(),
            target_transaction_id: result.transaction_id.clone(),
            idempotency_key: "undo-font".into(),
        })
        .unwrap();
    let undone = app
        .inspect_scene_project(&InspectProject {
            project_id: request.project_id.clone(),
            revision: None,
        })
        .unwrap();
    assert_eq!(undone.project, initial.project);
    app.apply_project_redo(&RedoRequest {
        project_id: request.project_id.clone(),
        expected_revision: undo.transaction.revision,
        undo_transaction_id: undo.transaction.transaction_id,
        idempotency_key: "redo-font".into(),
    })
    .unwrap();
    let current = app
        .inspect_scene_project(&InspectProject {
            project_id: request.project_id.clone(),
            revision: None,
        })
        .unwrap();
    assert_eq!(
        app.apply_scene_project(&ApplySceneProject {
            project_id: request.project_id.clone(),
            expected_revision: current.revision.clone(),
            idempotency_key: request.idempotency_key.clone(),
            label: "Different operation".into(),
            operations: vec![ProjectOperation::Rename {
                name: "Must not commit".into()
            }],
        })
        .unwrap_err()
        .code,
        "IDEMPOTENCY_KEY_REUSED"
    );
    let cancelled = ImportFont {
        idempotency_key: "cancelled-import".into(),
        font_id: "cancelled-font".into(),
        expected_revision: current.revision.clone(),
        ..request.clone()
    };
    let error = scene::import_font_resource(
        &mut app,
        &source,
        &FixtureInspector,
        &publisher,
        &cancelled,
        &policy,
        &FontImportPolicy::default(),
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
        app.inspect_scene_project(&InspectProject {
            project_id: request.project_id.clone(),
            revision: None
        })
        .unwrap(),
        current
    );
    let racing = ImportFont {
        idempotency_key: "racing-import".into(),
        font_id: "racing-font".into(),
        expected_revision: current.revision.clone(),
        ..request.clone()
    };
    let mut advanced = false;
    let error = scene::import_font_resource(
        &mut app,
        &source,
        &FixtureInspector,
        &publisher,
        &racing,
        &policy,
        &FontImportPolicy::default(),
        &mut || {
            if live.get() && !advanced {
                let mut owner = EditorApplication::open(&database)?;
                owner.apply_scene_project(&ApplySceneProject {
                    project_id: request.project_id.clone(),
                    expected_revision: current.revision.clone(),
                    idempotency_key: "advance-during-import".into(),
                    label: "Concurrent rename".into(),
                    operations: vec![ProjectOperation::Rename {
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
        .inspect_scene_project(&InspectProject {
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
        scene::import_font_resource(
            &mut app,
            &source,
            &FixtureInspector,
            &publisher,
            &request,
            &FontFetchPolicy::default(),
            &FontImportPolicy::default(),
            &mut || Ok(())
        )
        .unwrap(),
        result
    );
    assert_eq!(source.calls.get(), acquisitions);
    let read_recovery = ReadEditRecovery {
        project_id: request.project_id.clone(),
        recovery_id: Some(recovery.recovery_id.clone()),
    };
    let recovered = app
        .inspect_edit_recovery(&read_recovery)
        .unwrap()
        .record
        .unwrap();
    assert_eq!(recovered.context, recovery.context);
    assert_eq!(
        recovered.receipt,
        Some(EditRecoveryReceipt::FontImport {
            transaction: result.clone()
        })
    );
    // A matching idempotency row alone is insufficient: recovery also checks
    // the full original request, result and actual revision parent edge.
    let changed_journal = serde_json::to_string(&ImportFont {
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
        app.inspect_edit_recovery(&read_recovery).unwrap_err().code,
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
        app.inspect_edit_recovery(&read_recovery).unwrap_err().code,
        "STORAGE_ERROR"
    );
    app.connection
        .execute(
            "UPDATE revisions SET parent_revision=?1 WHERE revision=?2",
            params![result.base_revision, result.revision],
        )
        .unwrap();
    let saved = app
        .ack_edit_recovery(&AckEditRecovery {
            project_id: request.project_id.clone(),
            recovery_id: recovery.recovery_id,
        })
        .unwrap();
    assert_eq!(saved.resolution, EditRecoveryResolution::Saved);
    assert_eq!(
        saved.receipt,
        Some(EditRecoveryReceipt::FontImport {
            transaction: result.clone()
        })
    );
    assert!(
        app.inspect_edit_recovery(&read_recovery)
            .unwrap()
            .record
            .is_none()
    );
    assert_eq!(source.calls.get(), acquisitions);
    for changed in [
        ImportFont {
            required_text: "changed".into(),
            ..request.clone()
        },
        ImportFont {
            catalog_id: "google-mashanzheng-regular-v1".into(),
            ..request.clone()
        },
    ] {
        assert_eq!(
            scene::import_font_resource(
                &mut app,
                &source,
                &FixtureInspector,
                &publisher,
                &changed,
                &policy,
                &FontImportPolicy::default(),
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
        scene::import_font_resource(
            &mut app,
            &source,
            &FixtureInspector,
            &publisher,
            &request,
            &policy,
            &FontImportPolicy::default(),
            &mut || Ok(())
        )
        .unwrap_err()
        .code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(source.calls.get(), acquisitions);
}
