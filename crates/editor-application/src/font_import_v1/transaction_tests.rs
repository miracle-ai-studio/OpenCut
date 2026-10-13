use super::{catalog::*, *};
use crate::project_v4::*;
use opencut_contracts::{
    CreateProject, FrameRate, TransactionResult, font_import_v1::ImportFontV1, project_v4_ops::*,
};
use std::cell::Cell;

struct Store {
    source: StoredProjectRevisionV4,
    receipt: Option<(String, TransactionResult)>,
    reads: Cell<usize>,
    denied: bool,
}
fn fixture() -> (Store, ImportFontV1) {
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
    let source = StoredProjectRevisionV4 {
        document: StoredProjectDocumentV4::V4(Box::new(project)),
        revision: "fixed".into(),
    };
    (
        Store {
            source,
            receipt: None,
            reads: Cell::new(0),
            denied: false,
        },
        ImportFontV1 {
            project_id: "project".into(),
            expected_revision: "fixed".into(),
            idempotency_key: "import-key".into(),
            catalog_id: "google-lato-regular-v1".into(),
            font_id: "font".into(),
            required_text: "AV To".into(),
        },
    )
}
impl ProjectV4Read for Store {
    fn authorize_v4(&self, _: V4Intent<'_>) -> Result<(), EditorError> {
        if self.denied {
            Err(EditorError::new("NOT_AUTHORIZED", "revoked"))
        } else {
            Ok(())
        }
    }
    fn head_v4_source(&self, _: &str) -> Result<StoredProjectRevisionV4, EditorError> {
        self.reads.set(self.reads.get() + 1);
        Ok(self.source.clone())
    }
    fn revision_v4_source(&self, _: &str, _: &str) -> Result<StoredProjectRevisionV4, EditorError> {
        unreachable!()
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
    fn replay_v4_edit(
        &self,
        intent: V4Intent<'_>,
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
impl ProjectV4Store for Store {
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
        _: V4Intent<'_>,
        _: PreparedV4Commit,
    ) -> Result<TransactionResult, EditorError> {
        panic!("pre-acquisition failure cannot commit")
    }
}
struct NoNative;
impl FontSourceFetchV1 for NoNative {
    fn fetch(
        &self,
        _: &str,
        _: &FontFetchPolicyV1,
        _: &mut dyn FontImportCheckpointV1,
    ) -> Result<Vec<u8>, EditorError> {
        panic!("replay, denial or invalid fixed head must not fetch")
    }
}
impl FontInspectorV1 for NoNative {
    fn inspect(
        &self,
        _: &[u8],
        _: &str,
        _: &FontImportPolicyV1,
        _: &mut dyn FontImportCheckpointV1,
    ) -> Result<ParsedFontFaceV1, EditorError> {
        unreachable!()
    }
}
impl FontCasPublishV1 for NoNative {
    type Lease = ();
    fn publish(
        &self,
        _: &PreparedFontV1,
        _: &[u8],
        _: &mut dyn FontImportCheckpointV1,
    ) -> Result<(), EditorError> {
        unreachable!()
    }
}
fn call(store: &mut Store, request: &ImportFontV1) -> Result<TransactionResult, EditorError> {
    import_font_v1(
        store,
        &NoNative,
        &NoNative,
        &NoNative,
        request,
        &FontFetchPolicyV1::default(),
        &FontImportPolicyV1::default(),
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
        V4Intent::FontImport(&request)
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
        V4Intent::FontImport(&request)
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
        ImportFontV1 {
            required_text: "different".into(),
            ..request.clone()
        },
        ImportFontV1 {
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
