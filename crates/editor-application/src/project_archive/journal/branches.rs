//! Exact original branch journals. Stored labels never restore branch authority.
use super::historical::OriginalChange;
use super::*;
use opencut_contracts::branching::BranchCompareRequest;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
enum ApplyRecord {
    #[serde(rename = "branch_apply")]
    Base {
        branch_name: String,
        request: TransactionRequest,
    },
    #[serde(rename = "project_v3.branch_apply")]
    Visual {
        branch_name: String,
        request: ApplyHistoricalVisualProject,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ArchivedBranchApply(ApplyRecord);
impl ArchivedBranchApply {
    pub(super) fn scope<'a>(&'a self, project: &'a str) -> (&'a str, &'a str, Option<&'a str>) {
        match &self.0 {
            ApplyRecord::Base { request, .. } => (
                project,
                &request.expected_revision,
                Some(&request.idempotency_key),
            ),
            ApplyRecord::Visual { request, .. } => (
                &request.project_id,
                &request.expected_revision,
                Some(&request.idempotency_key),
            ),
        }
    }
    pub(super) fn validate(&self) -> Result<(), EditorError> {
        let name = match &self.0 {
            ApplyRecord::Base { branch_name, .. } | ApplyRecord::Visual { branch_name, .. } => {
                branch_name
            }
        };
        crate::branching::validate_name(name, false)
    }
    pub(super) fn validate_effect(
        &self,
        before: &ArchivedDocument,
        after: &ArchivedDocument,
    ) -> Result<(), EditorError> {
        let same = match &self.0 {
            ApplyRecord::Base { request, .. } => {
                let (ArchivedDocument::Legacy(before), ArchivedDocument::Legacy(after)) =
                    (before, after)
                else {
                    return Err(invalid(
                        "Original base branch edit changed its document shape",
                    ));
                };
                opencut_editor_core::plan_transaction(
                    &opencut_contracts::ProjectSnapshot {
                        project: before.clone(),
                        revision: request.expected_revision.clone(),
                    },
                    request,
                )?
                .proposed_project
                    == *after
            }
            ApplyRecord::Visual { request, .. } => {
                let (
                    ArchivedDocument::HistoricalVisual(before),
                    ArchivedDocument::HistoricalVisual(after),
                ) = (before, after)
                else {
                    return Err(invalid(
                        "Original visual branch edit changed its document shape",
                    ));
                };
                crate::historical_visual::plan_apply(before, request)?.proposed_project == *after
            }
        };
        if !same {
            return Err(invalid(
                "Original branch edit differs from its original kernel",
            ));
        }
        Ok(())
    }
    pub(super) fn remap(
        &mut self,
        project: &str,
        revisions: &BTreeMap<String, String>,
    ) -> Result<(), EditorError> {
        let expected = match &mut self.0 {
            ApplyRecord::Base { request, .. } => &mut request.expected_revision,
            ApplyRecord::Visual { request, .. } => {
                request.project_id = project.into();
                &mut request.expected_revision
            }
        };
        *expected = revisions
            .get(expected)
            .ok_or_else(|| invalid("Cannot map original branch edit revision"))?
            .clone();
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
enum MergeRecord {
    #[serde(rename = "branch_merge")]
    Base {
        source_branch: String,
        target_branch: String,
        base_revision: String,
        source_revision: String,
        expected_target_revision: String,
        idempotency_key: String,
        changes: Vec<OriginalChange>,
    },
    #[serde(rename = "project_v3.branch_merge")]
    Visual {
        source_branch: String,
        target_branch: String,
        base_revision: String,
        source_revision: String,
        changes: Vec<OriginalChange>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ArchivedBranchMerge(MergeRecord);
impl ArchivedBranchMerge {
    pub(super) fn scope<'a>(
        &'a self,
        project: &'a str,
        base: &'a str,
    ) -> (&'a str, &'a str, Option<&'a str>) {
        match &self.0 {
            MergeRecord::Base {
                expected_target_revision,
                idempotency_key,
                ..
            } => (project, expected_target_revision, Some(idempotency_key)),
            MergeRecord::Visual { .. } => (project, base, None),
        }
    }
    pub(super) fn references(&self) -> (&str, &str) {
        match &self.0 {
            MergeRecord::Base {
                base_revision,
                source_revision,
                ..
            }
            | MergeRecord::Visual {
                base_revision,
                source_revision,
                ..
            } => (base_revision, source_revision),
        }
    }
    pub(super) fn validate(&self) -> Result<(), EditorError> {
        let (source, target) = match &self.0 {
            MergeRecord::Base {
                source_branch,
                target_branch,
                ..
            }
            | MergeRecord::Visual {
                source_branch,
                target_branch,
                ..
            } => (source_branch, target_branch),
        };
        crate::branching::validate_name(source, true)?;
        crate::branching::validate_name(target, true)?;
        if source == target {
            return Err(invalid("Original merge branches must differ"));
        }
        Ok(())
    }
    pub(super) fn validate_effect(
        &self,
        project: &str,
        base: &str,
        after: &ArchivedDocument,
        revisions: &BTreeMap<&str, &ArchivedRevision>,
        merges: &BTreeMap<&str, &str>,
        newest: &[&str],
    ) -> Result<(), EditorError> {
        let (ancestor, source) = self.references();
        if super::super::archive_common_ancestor(source, base, revisions, merges, newest)?
            != ancestor
        {
            return Err(invalid(
                "Original merge base differs from the published DAG",
            ));
        }
        let (source_branch, target_branch) = match &self.0 {
            MergeRecord::Base {
                source_branch,
                target_branch,
                ..
            }
            | MergeRecord::Visual {
                source_branch,
                target_branch,
                ..
            } => (source_branch, target_branch),
        };
        let request = BranchCompareRequest {
            project_id: project.into(),
            source_branch: source_branch.clone(),
            target_branch: target_branch.clone(),
            expected_source_revision: source.into(),
            expected_target_revision: base.into(),
            base_asset_versions: BTreeMap::new(),
        };
        let before = &revisions[ancestor].document;
        let source = &revisions[source].document;
        let target = &revisions[base].document;
        let (expected, actual) = match &self.0 {
            MergeRecord::Base { changes, .. } => {
                let (
                    ArchivedDocument::Legacy(before),
                    ArchivedDocument::Legacy(source),
                    ArchivedDocument::Legacy(target),
                    ArchivedDocument::Legacy(after),
                ) = (before, source, target, after)
                else {
                    return Err(invalid("Original base merge changed its document shape"));
                };
                let plan = crate::collaboration::compare_legacy_branches(
                    &request, ancestor, before, source, target,
                );
                if plan.changes.is_empty()
                    || !plan.conflicts.is_empty()
                    || plan.proposed_project.as_ref() != Some(after)
                {
                    return Err(invalid(
                        "Original base merge differs from its original kernel",
                    ));
                }
                (
                    serde_json::to_value(plan.changes),
                    serde_json::to_value(changes),
                )
            }
            MergeRecord::Visual { changes, .. } => {
                let (
                    ArchivedDocument::HistoricalVisual(before),
                    ArchivedDocument::HistoricalVisual(source),
                    ArchivedDocument::HistoricalVisual(target),
                    ArchivedDocument::HistoricalVisual(after),
                ) = (before, source, target, after)
                else {
                    return Err(invalid("Original visual merge changed its document shape"));
                };
                let plan = crate::historical_visual_collaboration::compare_historical_branches(
                    &request, ancestor, before, source, target,
                );
                if plan.changes.is_empty()
                    || !plan.conflicts.is_empty()
                    || plan.proposed_project.as_ref() != Some(after)
                {
                    return Err(invalid(
                        "Original visual merge differs from its original kernel",
                    ));
                }
                (
                    serde_json::to_value(plan.changes),
                    serde_json::to_value(changes),
                )
            }
        };
        if expected.map_err(|_| invalid("Cannot encode original merge changes"))?
            != actual.map_err(|_| invalid("Cannot encode stored merge changes"))?
        {
            return Err(invalid(
                "Original merge changes differ from the original kernel",
            ));
        }
        Ok(())
    }
    pub(super) fn remap(
        &mut self,
        revisions: &BTreeMap<String, String>,
    ) -> Result<(), EditorError> {
        let map = |id: &mut String| -> Result<(), EditorError> {
            *id = revisions
                .get(id)
                .ok_or_else(|| invalid("Cannot map original merge revision"))?
                .clone();
            Ok(())
        };
        match &mut self.0 {
            MergeRecord::Base {
                base_revision,
                source_revision,
                expected_target_revision,
                ..
            } => {
                map(base_revision)?;
                map(source_revision)?;
                map(expected_target_revision)?;
            }
            MergeRecord::Visual {
                base_revision,
                source_revision,
                ..
            } => {
                map(base_revision)?;
                map(source_revision)?;
            }
        }
        Ok(())
    }
}
