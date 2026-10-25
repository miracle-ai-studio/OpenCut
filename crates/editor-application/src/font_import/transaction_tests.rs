use super::{catalog::*, *};
use crate::scene::*;
use opencut_contracts::{
    CreateProject, FrameRate, TransactionResult, font_import::ImportFont, scene_ops::*,
};
use std::cell::Cell;

struct Store {
    source: StoredProjectRevision,
    receipt: Option<(String, TransactionResult)>,
    reads: Cell<usize>,
    denied: bool,
}
fn fixture() -> (Store, ImportFont) {
    let project = new_document(&CreateProject {
        id: "project".into(),
        name: "Project".into(),
        sequence_id: "main".into(),
        width: 320,
        height: 180,
        fps: FrameRate {
            numerator: 24,
            denominator: 1,
        },
    })
    .unwrap();
    let source = StoredProjectRevision {
        document: StoredProjectDocument::Editable(Box::new(project)),
        revision: "fixed".into(),
    };
    (
        Store {
            source,
            receipt: None,
            reads: Cell::new(0),
            denied: false,
        },
        ImportFont {
            project_id: "project".into(),
            expected_revision: "fixed".into(),
            idempotency_key: "import-key".into(),
            catalog_id: "google-lato-regular-v1".into(),
            font_id: "font".into(),
            required_text: "AV To".into(),
        },
    )
}
impl SceneRead for Store {
    fn authorize_project(&self, _: SceneCommitIntent<'_>) -> Result<(), EditorError> {
        if self.denied {
            Err(EditorError::new("NOT_AUTHORIZED", "revoked"))
        } else {
            Ok(())
        }
    }
    fn head_scene_source(&self, _: &str) -> Result<StoredProjectRevision, EditorError> {
        self.reads.set(self.reads.get() + 1);
        Ok(self.source.clone())
    }
    fn revision_scene_source(
        &self,
        _: &str,
        _: &str,
    ) -> Result<StoredProjectRevision, EditorError> {
        unreachable!()
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
        intent: SceneCommitIntent<'_>,
    ) -> Result<Option<TransactionResult>, EditorError> {
        assert_eq!(intent.family(), "project_v4_font_import_v1");
        if let Some((digest, result)) = &self.receipt {
            if intent.identity()?.current_digest() != digest {
                return Err(EditorError::new(
                    "IDEMPOTENCY_KEY_REUSED",
                    "different original request",
                ));
            }
            return Ok(Some(result.clone()));
        }
        Ok(None)
    }
}
impl SceneStore for Store {
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
        _: SceneCommitIntent<'_>,
        _: PreparedSceneCommit,
    ) -> Result<TransactionResult, EditorError> {
        panic!("pre-acquisition failure cannot commit")
    }
}
struct NoNative;
impl FontSourceFetch for NoNative {
    fn fetch(
        &self,
        _: &str,
        _: &FontFetchPolicy,
        _: &mut dyn FontImportCheckpoint,
    ) -> Result<Vec<u8>, EditorError> {
        panic!("replay, denial or invalid fixed head must not fetch")
    }
}
impl FontInspector for NoNative {
    fn inspect(
        &self,
        _: &[u8],
        _: &str,
        _: &FontImportPolicy,
        _: &mut dyn FontImportCheckpoint,
    ) -> Result<ParsedFontFace, EditorError> {
        unreachable!()
    }
}
impl FontCasPublish for NoNative {
    type Lease = ();
    fn publish(
        &self,
        _: &PreparedFont,
        _: &[u8],
        _: &mut dyn FontImportCheckpoint,
    ) -> Result<(), EditorError> {
        unreachable!()
    }
}
fn call(store: &mut Store, request: &ImportFont) -> Result<TransactionResult, EditorError> {
    import_font_resource(
        store,
        &NoNative,
        &NoNative,
        &NoNative,
        request,
        &FontFetchPolicy::default(),
        &FontImportPolicy::default(),
        &mut || Ok(()),
    )
}

#[test]
fn authorized_lost_receipt_replay_is_offline_and_does_not_read_a_new_head() {
    let (mut store, request) = fixture();
    let transaction = TransactionResult {
        transaction_id: "transaction".into(),
        base_revision: "fixed".into(),
        revision: "committed".into(),
        created_ids: vec!["font".into()],
        changed_ids: vec![],
        deleted_ids: vec![],
    };
    store.receipt = Some((
        SceneCommitIntent::FontImport(&request)
            .identity()
            .unwrap()
            .current_digest()
            .into(),
        transaction.clone(),
    ));
    store.source.revision = "later-head".into();
    assert_eq!(call(&mut store, &request).unwrap(), transaction);
    assert_eq!(store.reads.get(), 0);
    store.denied = true;
    assert_eq!(
        call(&mut store, &request).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
}

#[test]
fn source_and_required_text_remain_in_original_request_identity() {
    let (mut store, request) = fixture();
    store.receipt = Some((
        SceneCommitIntent::FontImport(&request)
            .identity()
            .unwrap()
            .current_digest()
            .into(),
        TransactionResult {
            transaction_id: "transaction".into(),
            base_revision: "fixed".into(),
            revision: "committed".into(),
            created_ids: vec![],
            changed_ids: vec![],
            deleted_ids: vec![],
        },
    ));
    for changed in [
        ImportFont {
            required_text: "different".into(),
            ..request.clone()
        },
        ImportFont {
            catalog_id: "google-mashanzheng-regular-v1".into(),
            ..request.clone()
        },
    ] {
        assert_eq!(
            call(&mut store, &changed).unwrap_err().code,
            "IDEMPOTENCY_KEY_REUSED"
        );
    }
    assert_eq!(store.reads.get(), 0);
}

#[test]
fn moved_head_and_logical_id_collision_fail_before_downloading() {
    let (mut store, mut request) = fixture();
    store.source.revision = "moved".into();
    assert_eq!(
        call(&mut store, &request).unwrap_err().code,
        "REVISION_CONFLICT"
    );
    store.source.revision = "fixed".into();
    request.font_id = request.project_id.clone();
    assert!(call(&mut store, &request).is_err());
}
