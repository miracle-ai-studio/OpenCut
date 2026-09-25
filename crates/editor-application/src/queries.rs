//! Portable read use cases, history traversal and cursor contracts.
use crate::job_views::{project, public_error, valid_job_id};
use crate::projects::ProjectRead;
use crate::{JobEvent, JobRecord, RevisionPage, RevisionSummary, StoredEvent};
use opencut_api_contracts::job_views::*;
use opencut_contracts::{EditorError, ProjectSnapshot};

pub struct JobPageSlice {
    pub records: Vec<JobRecord>,
    pub upper_id: Option<String>,
    pub has_more: bool,
}
pub struct EventAnchor {
    pub sequence: i64,
    pub event_id: String,
}
/// Adapter returns a read-consistent page: validate anchors and read rows in
/// the same snapshot. Returned membership uses a fixed insertion upper bound;
/// task states are observations at the individual page's read time.
pub trait JobRead {
    fn read_job(&self, job_id: &str) -> Result<JobRecord, EditorError>;
    /// Read one authorized job and the cursor scope identity in one storage snapshot.
    fn read_job_for_cursor(&self, job_id: &str) -> Result<(JobRecord, String), EditorError>;
    fn read_jobs(&self) -> Result<Vec<JobRecord>, EditorError>;
    fn read_job_page(
        &self,
        upper: Option<&str>,
        after: Option<&str>,
        limit: usize,
    ) -> Result<JobPageSlice, EditorError>;
    fn read_job_changes(
        &self,
        anchor: Option<&EventAnchor>,
        job_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<JobEvent>, EditorError>;
    fn read_job_events(
        &self,
        after: i64,
        limit: usize,
        job_id: Option<&str>,
    ) -> Result<Vec<JobEvent>, EditorError>;
}
pub trait WorkspaceRead: ProjectRead {
    fn read_projects(&self) -> Result<Vec<ProjectSnapshot>, EditorError>;
    fn read_revision_summary(
        &self,
        project_id: &str,
        revision: &str,
    ) -> Result<RevisionSummary, EditorError>;
    fn read_project_events(
        &self,
        after: i64,
        limit: usize,
    ) -> Result<Vec<StoredEvent>, EditorError>;
}
fn error(code: &str, message: &str) -> EditorError {
    EditorError::new(code, message)
}
fn page_limit(limit: usize) -> Result<(), EditorError> {
    if !(1..=100).contains(&limit) {
        Err(error("INVALID_ARGUMENT", "Page limit must be 1..100"))
    } else {
        Ok(())
    }
}
pub fn job_view(store: &impl JobRead, id: &str) -> Result<JobView, EditorError> {
    store.read_job(id).map(project).map_err(public_error)
}
pub fn job_query(store: &impl JobRead, input: JobQueryInput) -> Result<JobPage, EditorError> {
    page_limit(input.limit)?;
    let anchors = if let Some(cursor) = input.cursor.as_deref() {
        let p: Vec<_> = cursor.split(':').collect();
        if p.len() != 3 || p[0] != "job-page" || !valid_job_id(p[1]) || !valid_job_id(p[2]) {
            return Err(error("INVALID_CURSOR", "Invalid task page cursor"));
        }
        Some((p[1], p[2]))
    } else {
        None
    };
    let page = store.read_job_page(anchors.map(|p| p.0), anchors.map(|p| p.1), input.limit)?;
    let next_cursor = if page.has_more {
        page.upper_id
            .as_ref()
            .zip(page.records.last())
            .map(|(upper, job)| format!("job-page:{upper}:{}", job.job_id))
    } else {
        None
    };
    Ok(JobPage {
        jobs: page.records.into_iter().map(project).collect(),
        next_cursor,
    })
}
pub fn job_changes(
    store: &impl JobRead,
    input: JobChangesInput,
) -> Result<JobChangesPage, EditorError> {
    page_limit(input.limit)?;
    let scope = input.job_id.as_deref().unwrap_or("all");
    let anchor = if let Some(cursor) = &input.cursor {
        let p: Vec<_> = cursor.split(':').collect();
        if p.len() != 4 || p[0] != "job-events" || p[1] != scope {
            return Err(error(
                "INVALID_CURSOR",
                "Cursor does not match the job event filter",
            ));
        }
        let sequence = p[2]
            .parse::<i64>()
            .ok()
            .filter(|n| *n > 0)
            .ok_or_else(|| error("INVALID_CURSOR", "Invalid event cursor position"))?;
        Some(EventAnchor {
            sequence,
            event_id: p[3].into(),
        })
    } else {
        None
    };
    let records = store.read_job_changes(anchor.as_ref(), input.job_id.as_deref(), input.limit)?;
    let next_cursor = records
        .last()
        .map(|e| format!("job-events:{scope}:{}:{}", e.sequence, e.event_id))
        .or(input.cursor);
    let events = records.into_iter().map(crate::job_views::change).collect();
    Ok(JobChangesPage {
        events,
        next_cursor,
    })
}
pub fn history(
    store: &impl WorkspaceRead,
    project_id: &str,
    from: Option<&str>,
    limit: usize,
) -> Result<RevisionPage, EditorError> {
    if !(1..=100).contains(&limit) {
        return Err(error("INVALID_ARGUMENT", "History limit must be 1..100"));
    }
    let head = store.head(project_id)?.revision;
    let mut cursor = Some(from.unwrap_or(&head).to_owned());
    let mut revisions = Vec::new();
    while let Some(revision) = cursor.take() {
        let row = store.read_revision_summary(project_id, &revision)?;
        cursor = row.parent_revision.clone();
        revisions.push(row);
        if revisions.len() == limit {
            break;
        }
    }
    Ok(RevisionPage {
        project_id: project_id.into(),
        head_revision: head,
        revisions,
        next_cursor: cursor,
    })
}
pub fn job_events(
    store: &impl JobRead,
    after: i64,
    limit: usize,
    job_id: Option<&str>,
) -> Result<Vec<JobEvent>, EditorError> {
    if after < 0 || !(1..=500).contains(&limit) {
        return Err(error(
            "INVALID_ARGUMENT",
            "after_sequence must be nonnegative; limit must be 1..500",
        ));
    }
    store.read_job_events(after, limit, job_id)
}
pub fn project_events(
    store: &impl WorkspaceRead,
    after: i64,
    limit: usize,
) -> Result<Vec<StoredEvent>, EditorError> {
    if !(1..=500).contains(&limit) {
        return Err(error(
            "INVALID_ARGUMENT",
            "event limit must be between 1 and 500",
        ));
    }
    store.read_project_events(after, limit)
}
