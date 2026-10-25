use super::*;

/// Native storage reads the original outer format exactly. Historical visual
/// and editable content must never be projected into a simpler document.
#[derive(Clone, Debug, PartialEq)]
pub enum StoredProjectDocument {
    Legacy(Box<ProjectDocument>),
    HistoricalVisual(Box<HistoricalVisualDocument>),
    Editable(Box<EditorDocument>),
}
impl StoredProjectDocument {
    pub fn project_id(&self) -> &str {
        match self {
            Self::Legacy(p) => &p.id,
            Self::HistoricalVisual(p) => &p.base.id,
            Self::Editable(p) => &p.id,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct StoredProjectRevision {
    pub document: StoredProjectDocument,
    pub revision: String,
}

#[derive(Clone, Copy, Debug)]
pub enum SceneCommitIntent<'a> {
    Create(&'a CreateProjectRequest),
    Copy(&'a CopySceneProject),
    Promote(&'a PromoteProject),
    Apply(&'a ApplySceneProject),
    Restore(&'a RestoreProject),
    PublicApply(&'a opencut_contracts::project_public::ApplyProject),
    BranchMerge(&'a opencut_contracts::branching::BranchMergeRequest),
    Undo(&'a opencut_contracts::project_history::UndoRequest),
    Redo(&'a opencut_contracts::project_history::RedoRequest),
    FontImport(&'a opencut_contracts::font_import::ImportFont),
    DerivedFontImport(&'a opencut_contracts::font_derived::ImportFont),
    CaptionGenerate(&'a opencut_api_contracts::subtitles::GenerateCaptions),
    CaptionImport(&'a opencut_api_contracts::subtitles::SubtitleImport),
    CatalogImport(&'a opencut_contracts::catalog_resources::ImportCatalogResource),
    CatalogGenerate(&'a opencut_contracts::catalog_resources::GenerateCatalogSound),
    BrandImport(&'a opencut_contracts::brand_resources::ImportBrandResource),
}
impl<'a> SceneCommitIntent<'a> {
    pub fn project_id(self) -> &'a str {
        match self {
            Self::Create(r) => &r.project.id,
            Self::Copy(r) => &r.project_id,
            Self::Promote(r) => &r.project_id,
            Self::Apply(r) => &r.project_id,
            Self::Restore(r) => &r.project_id,
            Self::PublicApply(r) => &r.project_id,
            Self::BranchMerge(r) => &r.project_id,
            Self::Undo(r) => &r.project_id,
            Self::Redo(r) => &r.project_id,
            Self::FontImport(r) => &r.project_id,
            Self::DerivedFontImport(r) => &r.project_id,
            Self::CaptionGenerate(r) => &r.project_id,
            Self::CaptionImport(r) => &r.project_id,
            Self::CatalogImport(r) => &r.project_id,
            Self::CatalogGenerate(r) => &r.project_id,
            Self::BrandImport(r) => &r.project_id,
        }
    }
    /// Publication target is request data,not a second editing implementation.
    pub fn target_branch(self) -> &'a str {
        match self {
            Self::PublicApply(r) => r.branch_name.as_deref().unwrap_or("main"),
            Self::BranchMerge(r) => &r.target_branch,
            Self::Promote(r) => r.branch_name.as_deref().unwrap_or("main"),
            Self::Restore(r) => r.branch_name.as_deref().unwrap_or("main"),
            _ => "main",
        }
    }
    pub fn family(self) -> &'static str {
        match self {
            Self::Create(_) => "project_v4_create",
            Self::Copy(_) => "project_v4_copy",
            Self::Promote(_) => "project_v4_promote",
            Self::Apply(_) => "project_v4_apply",
            Self::Restore(_) => "project_v4_restore",
            Self::PublicApply(_) => "project_v4_public_apply",
            Self::BranchMerge(_) => "project_branch_merge",
            Self::Undo(_) => "project_v4_undo",
            Self::Redo(_) => "project_v4_redo",
            Self::FontImport(_) => "project_v4_font_import_v1",
            Self::DerivedFontImport(_) => "project_v4_derived_font_import_v1",
            Self::CaptionGenerate(_) => "subtitle_generate_v4",
            Self::CaptionImport(_) => "subtitle_import_v4",
            Self::CatalogImport(_) => "catalog_resources_import_v1",
            Self::CatalogGenerate(_) => "catalog_resources_generate_v1",
            Self::BrandImport(_) => "brand_resources_import_v1",
        }
    }
    pub fn scope(self) -> &'a str {
        match self {
            Self::Create(_) | Self::Copy(_) => "workspace",
            _ => self.project_id(),
        }
    }
    pub fn key(self) -> &'a str {
        match self {
            Self::Create(r) => &r.idempotency_key,
            Self::Copy(r) => &r.idempotency_key,
            Self::Promote(r) => &r.idempotency_key,
            Self::Apply(r) => &r.idempotency_key,
            Self::Restore(r) => &r.idempotency_key,
            Self::PublicApply(r) => &r.idempotency_key,
            Self::BranchMerge(r) => &r.idempotency_key,
            Self::Undo(r) => &r.idempotency_key,
            Self::Redo(r) => &r.idempotency_key,
            Self::FontImport(r) => &r.idempotency_key,
            Self::DerivedFontImport(r) => &r.idempotency_key,
            Self::CaptionGenerate(r) => &r.idempotency_key,
            Self::CaptionImport(r) => &r.idempotency_key,
            Self::CatalogImport(r) => &r.idempotency_key,
            Self::CatalogGenerate(r) => &r.idempotency_key,
            Self::BrandImport(r) => &r.idempotency_key,
        }
    }
    pub fn expected_revision(self) -> Option<&'a str> {
        match self {
            Self::Create(_) | Self::Copy(_) => None,
            Self::Promote(r) => Some(&r.expected_revision),
            Self::Apply(r) => Some(&r.expected_revision),
            Self::Restore(r) => Some(&r.expected_revision),
            Self::PublicApply(r) => Some(&r.expected_revision),
            Self::BranchMerge(r) => Some(&r.expected_target_revision),
            Self::Undo(r) => Some(&r.expected_revision),
            Self::Redo(r) => Some(&r.expected_revision),
            Self::FontImport(r) => Some(&r.expected_revision),
            Self::DerivedFontImport(r) => Some(&r.expected_revision),
            Self::CaptionGenerate(r) => Some(&r.expected_revision),
            Self::CaptionImport(r) => Some(&r.expected_revision),
            Self::CatalogImport(r) => Some(&r.expected_revision),
            Self::CatalogGenerate(r) => Some(&r.expected_revision),
            Self::BrandImport(r) => Some(&r.expected_revision),
        }
    }
    pub fn identity(self) -> Result<crate::identity::RequestIdentity, EditorError> {
        match self {
            Self::CatalogImport(r) => {
                crate::identity::request_identity(self.family(), self.scope(), r)
            }
            Self::CatalogGenerate(r) => {
                crate::identity::request_identity(self.family(), self.scope(), r)
            }
            Self::BrandImport(r) => crate::brand_resources::request_identity(r),
            Self::Create(r) => crate::identity::request_identity(self.family(), self.scope(), r),
            Self::Copy(r) => crate::identity::request_identity(self.family(), self.scope(), r),
            Self::Promote(r) => crate::identity::request_identity(self.family(), self.scope(), r),
            Self::Apply(r) => crate::identity::request_identity(self.family(), self.scope(), r),
            Self::Restore(r) => crate::identity::request_identity(self.family(), self.scope(), r),
            Self::PublicApply(r) => {
                crate::identity::request_identity(self.family(), self.scope(), r)
            }
            Self::BranchMerge(r) => {
                crate::identity::request_identity(self.family(), self.scope(), r)
            }
            Self::Undo(r) => crate::identity::request_identity(self.family(), self.scope(), r),
            Self::Redo(r) => crate::identity::request_identity(self.family(), self.scope(), r),
            Self::CaptionGenerate(r) => {
                crate::identity::request_identity(self.family(), self.scope(), r)
            }
            Self::CaptionImport(r) => {
                crate::identity::request_identity(self.family(), self.scope(), r)
            }
            Self::FontImport(r) => {
                crate::identity::request_identity(self.family(), self.scope(), r)
            }
            Self::DerivedFontImport(r) => {
                crate::identity::request_identity(self.family(), self.scope(), r)
            }
        }
    }
    pub fn request_json(self) -> Result<String, EditorError> {
        match self {
            Self::CatalogImport(r) => encode_json(r, "catalog resource import request"),
            Self::CatalogGenerate(r) => encode_json(r, "local sound generation request"),
            Self::BrandImport(r) => encode_json(r, "pinned brand acquisition request"),
            Self::Create(r) => encode_json(r, "create request"),
            Self::Copy(r) => encode_json(r, "copy request"),
            Self::Promote(r) => encode_json(r, "promotion request"),
            Self::Apply(r) => encode_json(r, "edit request"),
            Self::Restore(r) => encode_json(r, "restore request"),
            Self::PublicApply(r) => encode_json(r, "Public edit request"),
            Self::BranchMerge(r) => encode_json(r, "branch merge request"),
            Self::Undo(r) => encode_json(r, "undo request"),
            Self::Redo(r) => encode_json(r, "redo request"),
            Self::FontImport(r) => encode_json(r, "font import request"),
            Self::DerivedFontImport(r) => encode_json(r, "derived font import request"),
            Self::CaptionGenerate(r) => encode_json(r, "caption generation request"),
            Self::CaptionImport(r) => encode_json(r, "subtitle import request"),
        }
    }
}

pub trait SceneRead {
    /// Current caller scope, action, write control and supplied resource scope
    /// must be checked BEFORE preparation/replay and again under commit lock.
    fn authorize_project(&self, intent: SceneCommitIntent<'_>) -> Result<(), EditorError>;
    /// Read-only proposals retain role/resource policy without acquiring a write hold.
    fn authorize_plan(&self, intent: SceneCommitIntent<'_>) -> Result<(), EditorError> {
        self.authorize_project(intent)
    }
    /// Reads enforce current Read authority and preserve fixed revision scope.
    fn head_scene_source(&self, project_id: &str) -> Result<StoredProjectRevision, EditorError>;
    /// Adapters with named refs resolve them in the same authorized read.
    fn head_scene_branch(
        &self,
        project_id: &str,
        branch: &str,
    ) -> Result<StoredProjectRevision, EditorError> {
        if branch == "main" {
            self.head_scene_source(project_id)
        } else {
            Err(EditorError::new("NOT_FOUND", "Named branch does not exist"))
        }
    }
    fn revision_scene_source(
        &self,
        project_id: &str,
        revision: &str,
    ) -> Result<StoredProjectRevision, EditorError>;
    /// Same scope/key across ANY operation family must reject a reused key.
    fn replay_scene_create(
        &self,
        request: &CreateProjectRequest,
    ) -> Result<Option<EditorSnapshot>, EditorError>;
    fn replay_scene_copy(
        &self,
        request: &CopySceneProject,
    ) -> Result<Option<EditorSnapshot>, EditorError>;
    fn replay_scene_edit(
        &self,
        intent: SceneCommitIntent<'_>,
    ) -> Result<Option<TransactionResult>, EditorError>;
}
pub trait SceneStore: SceneRead {
    /// Recheck authorization, replay and uniqueness; verify immutable resource
    /// bytes/scope under CAS protection. Head, first revision, receipt, outbox
    /// and audit must publish together or all roll back.
    fn create_scene_atomic(
        &mut self,
        request: &CreateProjectRequest,
        prepared: PreparedCreate,
    ) -> Result<EditorSnapshot, EditorError>;
    fn copy_scene_atomic(
        &mut self,
        request: &CopySceneProject,
        prepared: PreparedCreate,
    ) -> Result<EditorSnapshot, EditorError>;
    /// Hold CAS protection through SQL commit; recheck current authorization,
    /// write control, request identity/replay and expected head. Publish complete
    /// V4 document, transaction, receipt, outbox and audit atomically. Never
    /// re-plan against a newer head, and never downgrade head during restore.
    fn commit_scene_atomic(
        &mut self,
        intent: SceneCommitIntent<'_>,
        prepared: PreparedSceneCommit,
    ) -> Result<TransactionResult, EditorError>;
}

pub struct PreparedCreate {
    pub(super) snapshot: EditorSnapshot,
    pub(super) event_id: String,
    pub(super) request_json: String,
}
impl PreparedCreate {
    pub fn snapshot(&self) -> &EditorSnapshot {
        &self.snapshot
    }
    pub fn event_id(&self) -> &str {
        &self.event_id
    }
    pub fn request_json(&self) -> &str {
        &self.request_json
    }
}
pub struct PreparedSceneCommit {
    pub(super) journal_json: Option<String>,
    pub(super) snapshot: EditorSnapshot,
    pub(super) result: TransactionResult,
    pub(super) event_id: String,
    pub(super) request_json: String,
}
impl PreparedSceneCommit {
    pub fn journal_json(&self) -> &str {
        self.journal_json.as_deref().unwrap_or(&self.request_json)
    }
    pub fn snapshot(&self) -> &EditorSnapshot {
        &self.snapshot
    }
    pub fn result(&self) -> &TransactionResult {
        &self.result
    }
    pub fn event_id(&self) -> &str {
        &self.event_id
    }
    pub fn request_json(&self) -> &str {
        &self.request_json
    }
}
