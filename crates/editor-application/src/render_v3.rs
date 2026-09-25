//! Versioned preflight uses the same authority and public diagnostics as v2.
use crate::{
    access::SourceAccess,
    project_v3::AuthorizedSnapshotV3,
    render::{PreflightIssue, ProjectPreflightReport, RenderPreflightInput, public_issue_message},
};
use opencut_contracts::{EditorError, ProjectSnapshot, project_v3::ProjectSnapshotV3};

pub trait ProjectExecutionReadV3 {
    fn execution_snapshot_v3(
        &self,
        project: &str,
        revision: &str,
    ) -> Result<AuthorizedSnapshotV3, EditorError>;
}
pub struct FixedPreflightV3 {
    pub snapshot: ProjectSnapshotV3,
    pub input: RenderPreflightInput,
}
pub trait RenderPreflightV3 {
    fn inspect_v3(&self, request: FixedPreflightV3) -> Result<ProjectPreflightReport, EditorError>;
}
pub fn preflight(
    projects: &impl ProjectExecutionReadV3,
    backend: &impl RenderPreflightV3,
    input: RenderPreflightInput,
) -> Result<ProjectPreflightReport, EditorError> {
    let authorized = projects.execution_snapshot_v3(&input.project_id, &input.revision)?;
    if authorized.snapshot.project.base.id != input.project_id
        || authorized.snapshot.revision != input.revision
    {
        return Err(EditorError::new(
            "INTERNAL_ERROR",
            "Execution reader returned a different project or revision",
        ));
    }
    let restricted = authorized.sources == SourceAccess::ProjectContent;
    let base = ProjectSnapshot {
        project: authorized.snapshot.project.base.clone(),
        revision: input.revision.clone(),
    };
    let required = match opencut_editor_core::dependencies::required_asset_ids(
        &base,
        &input.sequence_id,
        opencut_editor_core::dependencies::AssetScope::All,
    ) {
        Ok(ids) => ids,
        Err(e) if e.code == "NOT_FOUND" => {
            return Ok(ProjectPreflightReport {
                revision: input.revision,
                sequence_id: input.sequence_id,
                ready: false,
                issues: vec![PreflightIssue {
                    code: "SEQUENCE_NOT_FOUND".into(),
                    message: public_issue_message("SEQUENCE_NOT_FOUND").into(),
                    asset_id: None,
                    clip_id: None,
                    cue_id: None,
                }],
            });
        }
        Err(e) => return Err(e),
    };
    if restricted {
        let issues: Vec<_> = base
            .project
            .assets
            .iter()
            .filter(|a| required.contains(&a.id) && a.version.is_none())
            .map(|a| PreflightIssue {
                code: "ASSET_LOCK_REQUIRED".into(),
                message: public_issue_message("ASSET_LOCK_REQUIRED").into(),
                asset_id: Some(a.id.clone()),
                clip_id: None,
                cue_id: None,
            })
            .collect();
        if !issues.is_empty() {
            return Ok(ProjectPreflightReport {
                revision: input.revision,
                sequence_id: input.sequence_id,
                ready: false,
                issues,
            });
        }
    }
    let mut report = backend
        .inspect_v3(FixedPreflightV3 {
            snapshot: authorized.snapshot,
            input: input.clone(),
        })
        .map_err(|e| {
            if restricted {
                EditorError::new(&e.code, public_issue_message(&e.code))
            } else {
                e
            }
        })?;
    if report.revision != input.revision
        || report.sequence_id != input.sequence_id
        || report.ready != report.issues.is_empty()
    {
        return Err(EditorError::new(
            "INTERNAL_ERROR",
            "Preflight adapter returned an inconsistent fixed-revision report",
        ));
    }
    if restricted {
        for issue in &mut report.issues {
            issue.message = public_issue_message(&issue.code).into();
        }
    }
    Ok(report)
}
