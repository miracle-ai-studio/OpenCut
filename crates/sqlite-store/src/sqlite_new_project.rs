//! Shared atomic first-revision publication for verified external imports.
use super::*;
use opencut_contracts::{ProjectDocument, historical_visual::HistoricalVisualDocument};
use opencut_editor_application::audit::ExecutionContext;
use rusqlite::Transaction;
use serde::Serialize;

pub(super) trait PublishedDocument: Serialize {
    fn project_id(&self) -> &str;
    fn project_name(&self) -> &str;
}
impl PublishedDocument for ProjectDocument {
    fn project_id(&self) -> &str {
        &self.id
    }
    fn project_name(&self) -> &str {
        &self.name
    }
}
impl PublishedDocument for HistoricalVisualDocument {
    fn project_id(&self) -> &str {
        &self.base.id
    }
    fn project_name(&self) -> &str {
        &self.base.name
    }
}

impl PublishedDocument for opencut_contracts::scene::EditorDocument {
    fn project_id(&self) -> &str {
        &self.id
    }
    fn project_name(&self) -> &str {
        &self.name
    }
}

pub(super) struct NewProjectPublication<'a, T: PublishedDocument> {
    pub document: &'a T,
    pub revision: &'a str,
    pub operation_family: &'a str,
    pub idempotency_key: &'a str,
    pub request_digest: &'a str,
    pub result_kind: &'a str,
    pub result_json: &'a str,
    pub event_type: &'a str,
    pub event_payload: &'a Value,
}

/// Caller holds an IMMEDIATE transaction and has checked exact replay.
pub(super) fn publish<T: PublishedDocument>(
    tx: &Transaction<'_>,
    context: &ExecutionContext,
    input: NewProjectPublication<'_, T>,
) -> Result<(), EditorError> {
    publish_document(
        tx,
        context,
        input.document,
        input.revision,
        input.event_type,
        input.event_payload,
    )?;
    tx.execute("INSERT INTO idempotency(operation_family,scope,idempotency_key,request_hash,result_kind,result_json)
                VALUES (?1,'workspace',?2,?3,?4,?5)",
        params![input.operation_family,input.idempotency_key,input.request_digest,input.result_kind,input.result_json])
        .map_err(idempotency_insert_error)?;
    Ok(())
}

pub(super) fn publish_document<T: PublishedDocument>(
    tx: &Transaction<'_>,
    context: &ExecutionContext,
    document: &T,
    revision: &str,
    event_type: &str,
    event_payload: &Value,
) -> Result<(), EditorError> {
    if tx
        .query_row(
            "SELECT 1 FROM projects WHERE project_id=?1",
            [document.project_id()],
            |_| Ok(()),
        )
        .optional()
        .map_err(storage_error)?
        .is_some()
    {
        return Err(EditorError::new(
            "ALREADY_EXISTS",
            "Target project already exists",
        ));
    }
    let document_json = encode_json(document, "imported project document")?;
    let payload_json = encode_json(event_payload, "project import event")?;
    tx.execute(
        "INSERT INTO projects(project_id,name,head_revision) VALUES (?1,?2,?3)",
        params![document.project_id(), document.project_name(), revision],
    )
    .map_err(constraint_or_storage)?;
    tx.execute(
        "INSERT INTO revisions(revision,project_id,parent_revision,transaction_id,document_json)
                VALUES (?1,?2,NULL,NULL,?3)",
        params![revision, document.project_id(), document_json],
    )
    .map_err(storage_error)?;
    let event_id = format!("evt_{}", Uuid::new_v4().simple());
    tx.execute(
        "INSERT INTO outbox(event_id,project_id,revision,event_type,payload_json)
                VALUES (?1,?2,?3,?4,?5)",
        params![
            event_id,
            document.project_id(),
            revision,
            event_type,
            payload_json
        ],
    )
    .map_err(storage_error)?;
    sqlite_audit::insert(
        tx,
        context,
        event_type,
        sqlite_audit::Links {
            project_id: Some(document.project_id()),
            revision: Some(revision),
            ..Default::default()
        },
    )?;
    Ok(())
}
