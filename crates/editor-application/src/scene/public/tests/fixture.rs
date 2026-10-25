//! In-memory application authority only; native tests establish durable policy
//! and CAS locking. Every selector read in these tests is explicitly counted.
use super::*;
use std::cell::Cell;
pub(super) struct Fixture {
    pub current: EditorSnapshot,
    pub fixed: BTreeMap<String, StoredProjectRevision>,
    pub media: Option<ProjectAsset>,
    pub deny: bool,
    pub selection_reads: Cell<usize>,
    pub commits: usize,
    pub journal: String,
    pub receipt: Option<(String, TransactionResult)>,
}
impl Fixture {
    pub fn new(snapshot: EditorSnapshot) -> Self {
        let source = StoredProjectRevision {
            document: StoredProjectDocument::Editable(Box::new(snapshot.project.clone())),
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
impl SceneRead for Fixture {
    fn authorize_project(&self, _: SceneCommitIntent<'_>) -> Result<(), EditorError> {
        if self.deny {
            Err(EditorError::new("NOT_AUTHORIZED", "Denied"))
        } else {
            Ok(())
        }
    }
    fn head_scene_source(&self, _: &str) -> Result<StoredProjectRevision, EditorError> {
        Ok(StoredProjectRevision {
            document: StoredProjectDocument::Editable(Box::new(self.current.project.clone())),
            revision: self.current.revision.clone(),
        })
    }
    fn revision_scene_source(
        &self,
        _: &str,
        revision: &str,
    ) -> Result<StoredProjectRevision, EditorError> {
        self.selection_reads.set(self.selection_reads.get() + 1);
        self.fixed
            .get(revision)
            .cloned()
            .ok_or_else(|| missing("revision"))
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
impl ProjectPublicResources for Fixture {
    fn authorized_project_media_scene(
        &self,
        _: &str,
        media_id: &str,
        version: &opencut_contracts::AssetVersion,
    ) -> Result<ProjectAsset, EditorError> {
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
impl SceneStore for Fixture {
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
        self.authorize_project(intent)?;
        assert!(matches!(intent, SceneCommitIntent::PublicApply(_)));
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
