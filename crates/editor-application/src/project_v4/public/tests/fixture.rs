//! In-memory application authority only; native tests establish durable policy
//! and CAS locking. Every selector read in these tests is explicitly counted.
use super::*;
use std::cell::Cell;
pub(super) struct Fixture {
    pub current: ProjectSnapshotV4,
    pub fixed: BTreeMap<String, StoredProjectRevisionV4>,
    pub media: Option<AssetV4>,
    pub deny: bool,
    pub selection_reads: Cell<usize>,
    pub commits: usize,
    pub journal: String,
    pub receipt: Option<(String, TransactionResult)>,
}
impl Fixture {
    pub fn new(snapshot: ProjectSnapshotV4) -> Self {
        let source = StoredProjectRevisionV4 {
            document: StoredProjectDocumentV4::V4(Box::new(snapshot.project.clone())),
            revision: snapshot.revision.clone(),
        };
        Self {
            fixed: BTreeMap::from([(snapshot.revision.clone(), source)]),
            current: snapshot,
            media: None,
            deny: false,
            selection_reads: Cell::new(0),
            commits: 0,
            journal: String::new(),
            receipt: None,
        }
    }
}
impl ProjectV4Read for Fixture {
    fn authorize_v4(&self, _: V4Intent<'_>) -> Result<(), EditorError> {
        if self.deny {
            Err(EditorError::new("NOT_AUTHORIZED", "Denied"))
        } else {
            Ok(())
        }
    }
    fn head_v4_source(&self, _: &str) -> Result<StoredProjectRevisionV4, EditorError> {
        Ok(StoredProjectRevisionV4 {
            document: StoredProjectDocumentV4::V4(Box::new(self.current.project.clone())),
            revision: self.current.revision.clone(),
        })
    }
    fn revision_v4_source(
        &self,
        _: &str,
        revision: &str,
    ) -> Result<StoredProjectRevisionV4, EditorError> {
        self.selection_reads.set(self.selection_reads.get() + 1);
        self.fixed
            .get(revision)
            .cloned()
            .ok_or_else(|| missing("revision"))
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
        match &self.receipt {
            Some((digest, result)) if intent.identity()?.matches(digest, None) => {
                Ok(Some(result.clone()))
            }
            Some(_) => Err(EditorError::new(
                "IDEMPOTENCY_KEY_REUSED",
                "Another public request",
            )),
            None => Ok(None),
        }
    }
}
impl ProjectV4PublicResources for Fixture {
    fn authorized_project_media_v4(
        &self,
        _: &str,
        media_id: &str,
        version: &opencut_contracts::AssetVersion,
    ) -> Result<AssetV4, EditorError> {
        self.selection_reads.set(self.selection_reads.get() + 1);
        if media_id != "available" {
            return Err(EditorError::new("NOT_AUTHORIZED", "No scoped observation"));
        }
        let value = self.media.clone().ok_or_else(|| missing("media"))?;
        if &value.version != version {
            return Err(EditorError::new("NOT_AUTHORIZED", "Identity differs"));
        }
        Ok(value)
    }
}
impl ProjectV4Store for Fixture {
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
        self.authorize_v4(intent)?;
        assert!(matches!(intent, V4Intent::PublicApply(_)));
        if intent.expected_revision() != Some(self.current.revision.as_str()) {
            return Err(EditorError::new("REVISION_CONFLICT", "Head moved"));
        }
        self.journal = prepared.request_json().into();
        self.current = prepared.snapshot().clone();
        let result = prepared.result().clone();
        self.receipt = Some((intent.identity()?.current_digest().into(), result.clone()));
        self.commits += 1;
        Ok(result)
    }
}
