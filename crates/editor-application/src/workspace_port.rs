//! Typed host-facing workspace services; no database connection or storage paths cross this port.
use crate::{
    JobEvent, JobRecord, StoredEvent, access::*, audit::ExecutionContext, denials::*, uploads::*,
};
use opencut_contracts::EditorError;

pub trait WorkspacePort: DenialStore {
    fn set_execution_context(&mut self, context: ExecutionContext);
    fn set_caller(&mut self, caller: Caller);
    fn authenticate(&self, secret: &str) -> Result<Caller, EditorError>;
    fn access_view(&self) -> Result<AccessView, EditorError>;
    fn access_audit_query(&self, input: AccessAuditQuery) -> Result<AccessAuditPage, EditorError>;
    fn principal_policy(&self, id: &str) -> Result<PrincipalView, EditorError>;
    fn set_principal_policy(
        &mut self,
        policy: PrincipalPolicy,
    ) -> Result<PrincipalView, EditorError>;
    fn revoke_credential(&mut self, id: &str) -> Result<(), EditorError>;
    fn issue_credential(&mut self, id: &str) -> Result<IssuedCredential, EditorError>;
    fn event_changes(
        &self,
        input: opencut_api_contracts::event_stream::EventChangesInput,
    ) -> Result<opencut_api_contracts::event_stream::EventChangesPage, EditorError>;
    fn events_after(
        &self,
        after_sequence: i64,
        limit: usize,
    ) -> Result<Vec<StoredEvent>, EditorError>;
    fn job_events_after(
        &self,
        after_sequence: i64,
        limit: usize,
        job_id: Option<&str>,
    ) -> Result<Vec<JobEvent>, EditorError>;
    fn artifact_job(&self, job_id: &str) -> Result<JobRecord, EditorError>;
    fn get_job(&self, job_id: &str) -> Result<JobRecord, EditorError>;
    fn job_view(
        &self,
        job_id: &str,
    ) -> Result<opencut_api_contracts::job_views::JobView, EditorError> {
        self.get_job(job_id)
            .map(crate::job_views::project)
            .map_err(crate::job_views::public_error)
    }
    fn repair_cancel_job(&mut self, job_id: &str) -> Result<JobRecord, EditorError>;
    fn authorize_chunk(&self, input: &ChunkDescriptor) -> Result<UploadView, EditorError>;
    /// Only for an already selected fixed resource stream; this verifies live
    /// read scope, not resource membership or CAS bytes. Resolve those first.
    fn revalidate_project_resource_read(
        &self,
        input: &opencut_contracts::project_resources::InspectResource,
    ) -> Result<(), EditorError>;
    fn project_resource(
        &self,
        input: &opencut_contracts::project_resources::InspectResource,
    ) -> Result<opencut_contracts::project_resources::ResourceView, EditorError>;
    fn project_media(&self, input: &ProjectMediaInput)
    -> Result<MediaImportJobResult, EditorError>;
}
