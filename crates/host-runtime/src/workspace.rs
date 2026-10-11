//! Host construction of opaque workspace sessions. Clients depend on the
//! application protocol through Deref, never on a SQLite facade or connection.
use opencut_contracts::EditorError;
use opencut_editor_application::workspace_port::WorkspacePort;
use opencut_editor_application::{
    JobEvent, JobRecord, StoredEvent, access::*, audit::ExecutionContext, denials::*, uploads::*,
};
use opencut_sqlite_store::EditorApplication;
use std::{
    ops::{Deref, DerefMut},
    path::Path,
};
pub struct Workspace {
    port: Box<dyn WorkspacePort + Send>,
}
impl Workspace {
    pub fn new(port: impl WorkspacePort + Send + 'static) -> Self {
        Self {
            port: Box::new(port),
        }
    }
    pub fn open_managed(path: impl AsRef<Path>) -> Result<Self, EditorError> {
        Ok(Self::new(SqliteWorkspace(EditorApplication::open_managed(
            path,
        )?)))
    }
    pub fn open_managed_repair(path: impl AsRef<Path>) -> Result<Self, EditorError> {
        Ok(Self::new(SqliteWorkspace(
            EditorApplication::open_managed_repair(path)?,
        )))
    }
}
impl Deref for Workspace {
    type Target = dyn WorkspacePort + Send;
    fn deref(&self) -> &Self::Target {
        self.port.as_ref()
    }
}
impl DerefMut for Workspace {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.port.as_mut()
    }
}
struct SqliteWorkspace(EditorApplication);
impl WorkspacePort for SqliteWorkspace {
    fn set_execution_context(&mut self, context: ExecutionContext) {
        self.0.set_execution_context(context)
    }
    fn set_caller(&mut self, caller: Caller) {
        self.0.set_caller(caller)
    }
    fn authenticate(&self, secret: &str) -> Result<Caller, EditorError> {
        self.0.authenticate(secret)
    }
    fn access_view(&self) -> Result<AccessView, EditorError> {
        self.0.access_view()
    }
    fn access_audit_query(&self, input: AccessAuditQuery) -> Result<AccessAuditPage, EditorError> {
        self.0.access_audit_query(input)
    }
    fn principal_policy(&self, id: &str) -> Result<PrincipalView, EditorError> {
        self.0.principal_policy(id)
    }
    fn set_principal_policy(
        &mut self,
        policy: PrincipalPolicy,
    ) -> Result<PrincipalView, EditorError> {
        self.0.set_principal_policy(policy)
    }
    fn revoke_credential(&mut self, id: &str) -> Result<(), EditorError> {
        self.0.revoke_credential(id)
    }
    fn issue_credential(&mut self, id: &str) -> Result<IssuedCredential, EditorError> {
        self.0.issue_credential(id)
    }
    fn event_changes(
        &self,
        input: opencut_api_contracts::event_stream::EventChangesInput,
    ) -> Result<opencut_api_contracts::event_stream::EventChangesPage, EditorError> {
        self.0.event_changes(input)
    }
    fn events_after(
        &self,
        after_sequence: i64,
        limit: usize,
    ) -> Result<Vec<StoredEvent>, EditorError> {
        self.0.events_after(after_sequence, limit)
    }
    fn job_events_after(
        &self,
        after_sequence: i64,
        limit: usize,
        job_id: Option<&str>,
    ) -> Result<Vec<JobEvent>, EditorError> {
        self.0.job_events_after(after_sequence, limit, job_id)
    }
    fn artifact_job(&self, job_id: &str) -> Result<JobRecord, EditorError> {
        self.0.artifact_job(job_id)
    }
    fn get_job(&self, job_id: &str) -> Result<JobRecord, EditorError> {
        self.0.get_job(job_id)
    }
    fn repair_cancel_job(&mut self, job_id: &str) -> Result<JobRecord, EditorError> {
        self.0.repair_cancel_job(job_id)
    }
    fn authorize_chunk(&self, input: &ChunkDescriptor) -> Result<UploadSession, EditorError> {
        self.0.authorize_chunk(input)
    }
    fn project_media(
        &self,
        input: &ProjectMediaInput,
    ) -> Result<MediaImportJobResult, EditorError> {
        self.0.project_media(input)
    }
}
impl DenialStore for Workspace {
    fn record_denial(&self, denial: &Denial<'_>) -> Result<(), EditorError> {
        self.port.record_denial(denial)
    }
    fn read_denials(&self, query: &DenialQuery) -> Result<DenialPage, EditorError> {
        self.port.read_denials(query)
    }
}
impl DenialStore for SqliteWorkspace {
    fn record_denial(&self, denial: &Denial<'_>) -> Result<(), EditorError> {
        self.0.record_denial(denial)
    }
    fn read_denials(&self, query: &DenialQuery) -> Result<DenialPage, EditorError> {
        self.0.read_denials(query)
    }
}
