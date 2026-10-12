//! Read-consistent, resumable event observations; observing never mutates a job.
use crate::{
    StoredEvent,
    queries::{EventAnchor, JobRead},
};
pub use opencut_api_contracts::event_stream::*;
use opencut_contracts::EditorError;

pub(crate) fn public_project_event(mut event: StoredEvent) -> StoredEvent {
    // Older V4 outbox rows contain the internal snapshot, including private
    // legacy provenance. Public observations use only authorized outer columns.
    if matches!(
        event.event_type.as_str(),
        "project.v4_created" | "project.v4_copied"
    ) {
        event.payload = serde_json::json!({"project_id":event.project_id,"revision":event.revision,"schema_version":4});
    }
    event
}

fn project_change(scope: &EventScope, event: StoredEvent) -> EventChange {
    let event = public_project_event(event);
    EventChange {
        cursor: cursor(scope, event.sequence, &event.event_id),
        event: EventData::Project(ProjectEvent {
            event_id: event.event_id,
            project_id: event.project_id,
            revision: event.revision,
            event_type: event.event_type,
            payload: event.payload,
        }),
    }
}

pub trait ProjectEventRead {
    /// Validate filter and anchor and read records in one storage snapshot.
    fn read_project_changes(
        &self,
        anchor: Option<&EventAnchor>,
        project_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<StoredEvent>, EditorError>;
}
fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_CURSOR", message)
}
fn cursor(scope: &EventScope, sequence: i64, event_id: &str) -> String {
    let bytes = serde_json::to_vec(&(scope, sequence.to_string(), event_id))
        .expect("serializable event cursor");
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("events-v1:{hex}")
}
fn parse_cursor(value: &str, scope: &EventScope) -> Result<EventAnchor, EditorError> {
    let hex = value
        .strip_prefix("events-v1:")
        .filter(|v| v.len() <= 16_384 && v.len() % 2 == 0)
        .ok_or_else(|| invalid("Invalid event cursor"))?;
    let bytes = hex
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let s = std::str::from_utf8(pair).map_err(|_| invalid("Invalid cursor encoding"))?;
            u8::from_str_radix(s, 16).map_err(|_| invalid("Invalid cursor encoding"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let (stored, position, event_id): (EventScope, String, String) =
        serde_json::from_slice(&bytes).map_err(|_| invalid("Invalid cursor contents"))?;
    if &stored != scope {
        return Err(invalid("Cursor does not match the event stream and filter"));
    }
    let sequence = position
        .parse::<i64>()
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(|| invalid("Invalid cursor position"))?;
    Ok(EventAnchor { sequence, event_id })
}
pub fn changes(
    store: &(impl ProjectEventRead + JobRead),
    input: EventChangesInput,
) -> Result<EventChangesPage, EditorError> {
    if !(1..=500).contains(&input.limit) {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Event page limit must be 1..500",
        ));
    }
    let filter = match &input.scope {
        EventScope::Projects { project_id } => project_id,
        EventScope::Jobs { job_id } => job_id,
    };
    if filter
        .as_ref()
        .is_some_and(|id| id.is_empty() || id.len() > 1024)
    {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Event filter must be a nonempty identifier of at most 1024 bytes",
        ));
    }
    let anchor = input
        .cursor
        .as_deref()
        .map(|value| parse_cursor(value, &input.scope))
        .transpose()?;
    let events = match &input.scope {
        EventScope::Projects { project_id } => store
            .read_project_changes(anchor.as_ref(), project_id.as_deref(), input.limit)?
            .into_iter()
            .map(|event| project_change(&input.scope, event))
            .collect::<Vec<_>>(),
        EventScope::Jobs { job_id } => store
            .read_job_changes(anchor.as_ref(), job_id.as_deref(), input.limit)?
            .into_iter()
            .map(|e| EventChange {
                cursor: cursor(&input.scope, e.sequence, &e.event_id),
                event: EventData::Job(crate::job_views::change(e)),
            })
            .collect(),
    };
    let next_cursor = events.last().map(|e| e.cursor.clone()).or(input.cursor);
    Ok(EventChangesPage {
        events,
        next_cursor,
    })
}

#[cfg(test)]
mod tests;
