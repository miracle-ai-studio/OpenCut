use super::*;

/// Native storage decodes the OUTER schema exactly. No projection of a V3/V4
/// head into frozen V2 DTOs may enter this port.
#[derive(Clone, Debug, PartialEq)]
pub enum StoredProjectDocumentV4 {
    Legacy(Box<ProjectDocument>),
    V3(Box<ProjectDocumentV3>),
    V4(Box<ProjectDocumentV4>),
}
impl StoredProjectDocumentV4 {
    pub fn project_id(&self) -> &str {
        match self {
            Self::Legacy(p) => &p.id,
            Self::V3(p) => &p.base.id,
            Self::V4(p) => &p.id,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct StoredProjectRevisionV4 {
    pub document: StoredProjectDocumentV4,
    pub revision: String,
}

#[derive(Clone, Copy, Debug)]
pub enum V4Intent<'a> {
    Create(&'a CreateV4Project),
    Copy(&'a CopyV4Project),
    Promote(&'a PromoteV4Project),
    Apply(&'a ApplyV4Project),
    Restore(&'a RestoreV4Revision),
    PublicApply(&'a opencut_contracts::project_v4_public::ApplyPublicV4Project),
    Undo(&'a opencut_contracts::project_v4_history::UndoV4Project),
    Redo(&'a opencut_contracts::project_v4_history::RedoV4Project),
    FontImport(&'a opencut_contracts::font_import_v1::ImportFontV1),
    DerivedFontImport(&'a opencut_contracts::font_derived_v1::ImportDerivedFontV1),
    CaptionGenerate(&'a opencut_api_contracts::subtitle_v4::GenerateCaptionsV4),
    CaptionImport(&'a opencut_api_contracts::subtitle_v4::SubtitleImportV4),
}
impl<'a> V4Intent<'a> {
    pub fn project_id(self) -> &'a str {
        match self {
            Self::Create(r) => &r.project.id,
            Self::Copy(r) => &r.project_id,
            Self::Promote(r) => &r.project_id,
            Self::Apply(r) => &r.project_id,
            Self::Restore(r) => &r.project_id,
            Self::PublicApply(r) => &r.project_id,
            Self::Undo(r) => &r.project_id,
            Self::Redo(r) => &r.project_id,
            Self::FontImport(r) => &r.project_id,
            Self::DerivedFontImport(r) => &r.project_id,
            Self::CaptionGenerate(r) => &r.project_id,
            Self::CaptionImport(r) => &r.project_id,
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
            Self::Undo(_) => "project_v4_undo",
            Self::Redo(_) => "project_v4_redo",
            Self::FontImport(_) => "project_v4_font_import_v1",
            Self::DerivedFontImport(_) => "project_v4_derived_font_import_v1",
            Self::CaptionGenerate(_) => "subtitle_generate_v4",
            Self::CaptionImport(_) => "subtitle_import_v4",
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
            Self::Undo(r) => &r.idempotency_key,
            Self::Redo(r) => &r.idempotency_key,
            Self::FontImport(r) => &r.idempotency_key,
            Self::DerivedFontImport(r) => &r.idempotency_key,
            Self::CaptionGenerate(r) => &r.idempotency_key,
            Self::CaptionImport(r) => &r.idempotency_key,
        }
    }
    pub fn expected_revision(self) -> Option<&'a str> {
        match self {
            Self::Create(_) | Self::Copy(_) => None,
            Self::Promote(r) => Some(&r.expected_revision),
            Self::Apply(r) => Some(&r.expected_revision),
            Self::Restore(r) => Some(&r.expected_revision),
            Self::PublicApply(r) => Some(&r.expected_revision),
            Self::Undo(r) => Some(&r.expected_revision),
            Self::Redo(r) => Some(&r.expected_revision),
            Self::FontImport(r) => Some(&r.expected_revision),
            Self::DerivedFontImport(r) => Some(&r.expected_revision),
            Self::CaptionGenerate(r) => Some(&r.expected_revision),
            Self::CaptionImport(r) => Some(&r.expected_revision),
        }
    }
    pub fn identity(self) -> Result<crate::identity::RequestIdentity, EditorError> {
        match self {
            Self::Create(r) => crate::identity::request_identity(self.family(), self.scope(), r),
            Self::Copy(r) => crate::identity::request_identity(self.family(), self.scope(), r),
            Self::Promote(r) => crate::identity::request_identity(self.family(), self.scope(), r),
            Self::Apply(r) => crate::identity::request_identity(self.family(), self.scope(), r),
            Self::Restore(r) => crate::identity::request_identity(self.family(), self.scope(), r),
            Self::PublicApply(r) => {
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
            Self::Create(r) => encode_json(r, "V4 create request"),
            Self::Copy(r) => encode_json(r, "V4 copy request"),
            Self::Promote(r) => encode_json(r, "V4 promotion request"),
            Self::Apply(r) => encode_json(r, "V4 edit request"),
            Self::Restore(r) => encode_json(r, "V4 restore request"),
            Self::PublicApply(r) => encode_json(r, "Public V4 edit request"),
            Self::Undo(r) => encode_json(r, "V4 undo request"),
            Self::Redo(r) => encode_json(r, "V4 redo request"),
            Self::FontImport(r) => encode_json(r, "V1 font import request"),
            Self::DerivedFontImport(r) => encode_json(r, "V1 derived font import request"),
            Self::CaptionGenerate(r) => encode_json(r, "V4 caption generation request"),
            Self::CaptionImport(r) => encode_json(r, "V4 subtitle import request"),
        }
    }
}

pub trait ProjectV4Read {
    /// Current caller scope, action, write control and supplied resource scope
    /// must be checked BEFORE preparation/replay and again under commit lock.
    fn authorize_v4(&self, intent: V4Intent<'_>) -> Result<(), EditorError>;
    /// Reads enforce current Read authority and preserve fixed revision scope.
    fn head_v4_source(&self, project_id: &str) -> Result<StoredProjectRevisionV4, EditorError>;
    fn revision_v4_source(
        &self,
        project_id: &str,
        revision: &str,
    ) -> Result<StoredProjectRevisionV4, EditorError>;
    /// Same scope/key across ANY operation family must reject a reused key.
    fn replay_v4_create(
        &self,
        request: &CreateV4Project,
    ) -> Result<Option<ProjectSnapshotV4>, EditorError>;
    fn replay_v4_copy(
        &self,
        request: &CopyV4Project,
    ) -> Result<Option<ProjectSnapshotV4>, EditorError>;
    fn replay_v4_edit(
        &self,
        intent: V4Intent<'_>,
    ) -> Result<Option<TransactionResult>, EditorError>;
}
pub trait ProjectV4Store: ProjectV4Read {
    /// Recheck authorization, replay and uniqueness; verify immutable resource
    /// bytes/scope under CAS protection. Head, first revision, receipt, outbox
    /// and audit must publish together or all roll back.
    fn create_v4_atomic(
        &mut self,
        request: &CreateV4Project,
        prepared: PreparedV4Create,
    ) -> Result<ProjectSnapshotV4, EditorError>;
    fn copy_v4_atomic(
        &mut self,
        request: &CopyV4Project,
        prepared: PreparedV4Create,
    ) -> Result<ProjectSnapshotV4, EditorError>;
    /// Hold CAS protection through SQL commit; recheck current authorization,
    /// write control, request identity/replay and expected head. Publish complete
    /// V4 document, transaction, receipt, outbox and audit atomically. Never
    /// re-plan against a newer head, and never downgrade head during restore.
    fn commit_v4_atomic(
        &mut self,
        intent: V4Intent<'_>,
        prepared: PreparedV4Commit,
    ) -> Result<TransactionResult, EditorError>;
}
pub trait ProjectV4LibraryRead {
    /// Native read snapshot owns cursor/principal binding, current visibility,
    /// stable page membership and exact outer-schema identification.
    fn list_v4_projects(&self, request: &ListV4Projects) -> Result<V4ProjectPage, EditorError>;
}

pub struct PreparedV4Create {
    pub(super) snapshot: ProjectSnapshotV4,
    pub(super) event_id: String,
    pub(super) request_json: String,
}
impl PreparedV4Create {
    pub fn snapshot(&self) -> &ProjectSnapshotV4 {
        &self.snapshot
    }
    pub fn event_id(&self) -> &str {
        &self.event_id
    }
    pub fn request_json(&self) -> &str {
        &self.request_json
    }
}
pub struct PreparedV4Commit {
    pub(super) journal_json: Option<String>,
    pub(super) snapshot: ProjectSnapshotV4,
    pub(super) result: TransactionResult,
    pub(super) event_id: String,
    pub(super) request_json: String,
}
impl PreparedV4Commit {
    pub fn journal_json(&self) -> &str {
        self.journal_json.as_deref().unwrap_or(&self.request_json)
    }
    pub fn snapshot(&self) -> &ProjectSnapshotV4 {
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
