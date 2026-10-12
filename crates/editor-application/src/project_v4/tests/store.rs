use super::*;
use std::cell::Cell;

#[derive(Default)]
pub(super) struct MemoryStore {
    pub heads: BTreeMap<String, StoredProjectRevisionV4>,
    pub revisions: BTreeMap<String, StoredProjectRevisionV4>,
    receipts: BTreeMap<(String, String), (String, String, serde_json::Value)>,
    pub deny: bool,
    pub deny_commit: bool,
    pub reads: Cell<usize>,
    pub publications: usize,
}
impl MemoryStore {
    pub fn with_source(source: StoredProjectRevisionV4) -> Self {
        let mut store = Self::default();
        store
            .heads
            .insert(source.document.project_id().into(), source.clone());
        store.revisions.insert(source.revision.clone(), source);
        store
    }
    fn receipt<T: serde::de::DeserializeOwned>(
        &self,
        intent: V4Intent<'_>,
    ) -> Result<Option<T>, EditorError> {
        let Some((family, digest, value)) = self
            .receipts
            .get(&(intent.scope().into(), intent.key().into()))
        else {
            return Ok(None);
        };
        if family != intent.family() || !intent.identity()?.matches(digest, None) {
            return Err(EditorError::new(
                "IDEMPOTENCY_KEY_REUSED",
                "Key belongs to another request",
            ));
        }
        serde_json::from_value(value.clone())
            .map(Some)
            .map_err(|e| invalid(e.to_string()))
    }
    fn publish<T: serde::Serialize>(
        &mut self,
        intent: V4Intent<'_>,
        snapshot: &ProjectSnapshotV4,
        result: &T,
    ) -> Result<(), EditorError> {
        let source = StoredProjectRevisionV4 {
            document: StoredProjectDocumentV4::V4(Box::new(snapshot.project.clone())),
            revision: snapshot.revision.clone(),
        };
        self.receipts.insert(
            (intent.scope().into(), intent.key().into()),
            (
                intent.family().into(),
                intent.identity()?.current_digest().into(),
                serde_json::to_value(result).unwrap(),
            ),
        );
        self.heads
            .insert(snapshot.project.id.clone(), source.clone());
        self.revisions.insert(snapshot.revision.clone(), source);
        self.publications += 1;
        Ok(())
    }
    fn new_atomic(
        &mut self,
        intent: V4Intent<'_>,
        prepared: PreparedV4Create,
    ) -> Result<ProjectSnapshotV4, EditorError> {
        self.authorize_v4(intent)?;
        if self.deny_commit {
            return Err(EditorError::new(
                "ACCESS_DENIED",
                "Commit permission revoked",
            ));
        }
        if let Some(replay) = self.receipt(intent)? {
            return Ok(replay);
        }
        if self.heads.contains_key(intent.project_id()) {
            return Err(EditorError::new("ALREADY_EXISTS", "Project exists"));
        }
        self.publish(intent, &prepared.snapshot, &prepared.snapshot)?;
        Ok(prepared.snapshot)
    }
}
impl ProjectV4Read for MemoryStore {
    fn authorize_v4(&self, _: V4Intent<'_>) -> Result<(), EditorError> {
        if self.deny {
            Err(EditorError::new("ACCESS_DENIED", "Denied"))
        } else {
            Ok(())
        }
    }
    fn head_v4_source(&self, id: &str) -> Result<StoredProjectRevisionV4, EditorError> {
        self.reads.set(self.reads.get() + 1);
        self.heads
            .get(id)
            .cloned()
            .ok_or_else(|| missing("project"))
    }
    fn revision_v4_source(
        &self,
        id: &str,
        revision: &str,
    ) -> Result<StoredProjectRevisionV4, EditorError> {
        self.reads.set(self.reads.get() + 1);
        self.revisions
            .get(revision)
            .filter(|s| s.document.project_id() == id)
            .cloned()
            .ok_or_else(|| missing("revision"))
    }
    fn replay_v4_create(
        &self,
        request: &CreateV4Project,
    ) -> Result<Option<ProjectSnapshotV4>, EditorError> {
        self.receipt(V4Intent::Create(request))
    }
    fn replay_v4_copy(
        &self,
        request: &CopyV4Project,
    ) -> Result<Option<ProjectSnapshotV4>, EditorError> {
        self.receipt(V4Intent::Copy(request))
    }
    fn replay_v4_edit(
        &self,
        intent: V4Intent<'_>,
    ) -> Result<Option<TransactionResult>, EditorError> {
        self.receipt(intent)
    }
}
impl ProjectV4Store for MemoryStore {
    fn create_v4_atomic(
        &mut self,
        request: &CreateV4Project,
        prepared: PreparedV4Create,
    ) -> Result<ProjectSnapshotV4, EditorError> {
        self.new_atomic(V4Intent::Create(request), prepared)
    }
    fn copy_v4_atomic(
        &mut self,
        request: &CopyV4Project,
        prepared: PreparedV4Create,
    ) -> Result<ProjectSnapshotV4, EditorError> {
        self.new_atomic(V4Intent::Copy(request), prepared)
    }
    fn commit_v4_atomic(
        &mut self,
        intent: V4Intent<'_>,
        prepared: PreparedV4Commit,
    ) -> Result<TransactionResult, EditorError> {
        self.authorize_v4(intent)?;
        if self.deny_commit {
            return Err(EditorError::new(
                "ACCESS_DENIED",
                "Commit permission revoked",
            ));
        }
        if let Some(replay) = self.receipt(intent)? {
            return Ok(replay);
        }
        let current = self.head_v4_source(intent.project_id())?;
        if intent.expected_revision() != Some(current.revision.as_str())
            || current.revision != prepared.result.base_revision
        {
            return Err(EditorError::new("REVISION_CONFLICT", "Head advanced"));
        }
        self.publish(intent, &prepared.snapshot, &prepared.result)?;
        Ok(prepared.result)
    }
}
