//! Shared trusted fixed-revision job authorization; policy remains typed per executor.
use super::*;
use opencut_contracts::project_v4::ProjectSnapshotV4;
use opencut_editor_application::{
    access::{Caller, VerifiedCredential},
    audit::ExecutionContext,
    project_v4::StoredProjectDocumentV4,
};
use opencut_job_execution::project_v4_audio::TrustedJobAuthorityV1;
pub(super) fn authority(context: &ExecutionContext) -> TrustedJobAuthorityV1 {
    match context.caller() {
        Caller::LocalOwner => TrustedJobAuthorityV1::LocalOwner,
        Caller::Credential(c) => TrustedJobAuthorityV1::Credential {
            principal_id: c.principal_id().into(),
            credential_id: c.credential_id().into(),
        },
    }
}
pub(super) fn context(value: &TrustedJobAuthorityV1) -> ExecutionContext {
    ExecutionContext::for_caller(
        opencut_editor_application::audit::CallSource::Worker,
        match value {
            TrustedJobAuthorityV1::LocalOwner => Caller::LocalOwner,
            TrustedJobAuthorityV1::Credential {
                principal_id,
                credential_id,
            } => Caller::Credential(VerifiedCredential::from_verified(
                principal_id.clone(),
                credential_id.clone(),
            )),
        },
    )
}
pub(super) fn fixed(
    connection: &Connection,
    project: &str,
    revision: &str,
) -> Result<ProjectSnapshotV4, EditorError> {
    let stored = sqlite_project_v4::load_source(connection, project, Some(revision))?;
    let StoredProjectDocumentV4::V4(document) = stored.document else {
        return Err(EditorError::new(
            "UNSUPPORTED_PROJECT_VERSION",
            "V4 rendering requires a schema 4 revision",
        ));
    };
    Ok(ProjectSnapshotV4 {
        project: *document,
        revision: stored.revision,
    })
}
pub(super) fn require_scope(
    connection: &Connection,
    caller: &ExecutionContext,
    project: &str,
) -> Result<(), EditorError> {
    sqlite_access::require(connection, caller, project, ProjectAction::Read)?;
    sqlite_access::require(connection, caller, project, ProjectAction::RunJobs)
}
pub(super) fn revalidate(
    app: &EditorApplication,
    kind: &str,
    job_id: &str,
    worker: &str,
    attempt: u32,
    selected_authority: &TrustedJobAuthorityV1,
    policy: &impl serde::Serialize,
) -> Result<(), EditorError> {
    let tx = app
        .connection
        .unchecked_transaction()
        .map_err(storage_error)?;
    let row: (String, String, String, u32, Option<String>, Option<i64>, bool, String, String) = tx.query_row(
            "SELECT p.project_id,p.revision,j.state,j.attempt,j.lease_owner,j.lease_until,j.cancel_requested,json_extract(j.input_json,'$.authority'),json_extract(j.input_json,'$.processing_policy') FROM jobs j JOIN job_projects p USING(job_id) JOIN revisions r ON r.project_id=p.project_id AND r.revision=p.revision WHERE j.job_id=?1 AND j.kind=?2",
            params![job_id, kind], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?,r.get(8)?))).map_err(storage_error)?;
    let now = lifecycle::JobClock::now_ms(&sqlite_jobs::SystemClock)?;
    if row.2 != "running"
        || row.3 != attempt
        || row.4.as_deref() != Some(worker)
        || row.5.is_none_or(|until| until <= now)
    {
        return Err(EditorError::new(
            "JOB_LEASE_LOST",
            "V4 render attempt is no longer current",
        ));
    }
    if row.6 {
        return Err(EditorError::new(
            "JOB_CANCELLED",
            "V4 render task was cancelled",
        ));
    }
    let original: TrustedJobAuthorityV1 = decode_json(&row.7, "V4 authority")?;
    let processing: Value = decode_json(&row.8, "V4 processing policy")?;
    if &original != selected_authority
        || processing != serde_json::to_value(policy).map_err(|_| access::denied())?
    {
        return Err(access::denied());
    }
    require_scope(&tx, &context(&original), &row.0)
}
