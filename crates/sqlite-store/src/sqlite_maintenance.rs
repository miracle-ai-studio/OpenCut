use super::*;
use opencut_editor_application::maintenance::{self, *};

// Native storage selection stays out of the application protocol.
trait PublicationTable: PublicationObject {
    const TABLE_PREFIX: &'static str;
}
impl PublicationTable for MaintenanceObject {
    const TABLE_PREFIX: &'static str = "maintenance";
}
impl PublicationTable for plugin_publication::Object {
    const TABLE_PREFIX: &'static str = "plugin";
}

pub(super) const MIGRATION: &str = "
CREATE TABLE maintenance_publications(publication_id TEXT PRIMARY KEY,object_json TEXT NOT NULL,intent_json TEXT NOT NULL,origin_request_id TEXT NOT NULL,pending INTEGER NOT NULL);
CREATE INDEX maintenance_pending_idx ON maintenance_publications(object_json,pending);
CREATE TABLE maintenance_events(sequence INTEGER PRIMARY KEY AUTOINCREMENT,publication_id TEXT NOT NULL REFERENCES maintenance_publications(publication_id),request_id TEXT NOT NULL,origin_request_id TEXT NOT NULL,object_json TEXT NOT NULL,event_json TEXT NOT NULL,created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
CREATE INDEX maintenance_object_idx ON maintenance_events(object_json,sequence);
CREATE INDEX maintenance_publication_idx ON maintenance_events(publication_id,sequence);
CREATE INDEX maintenance_request_idx ON maintenance_events(request_id,sequence);
CREATE INDEX maintenance_origin_idx ON maintenance_events(origin_request_id,sequence);
UPDATE metadata SET value='6' WHERE key='schema_version' AND value='5';
";
pub(super) const MIGRATION7:&str="
CREATE TABLE plugin_publications(publication_id TEXT PRIMARY KEY,object_json TEXT NOT NULL,intent_json TEXT NOT NULL,origin_request_id TEXT NOT NULL,pending INTEGER NOT NULL);
CREATE INDEX plugin_pending_idx ON plugin_publications(object_json,pending);
CREATE TABLE plugin_events(sequence INTEGER PRIMARY KEY AUTOINCREMENT,publication_id TEXT NOT NULL REFERENCES plugin_publications(publication_id),request_id TEXT NOT NULL,origin_request_id TEXT NOT NULL,object_json TEXT NOT NULL,event_json TEXT NOT NULL,created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
CREATE INDEX plugin_object_idx ON plugin_events(object_json,sequence);
CREATE INDEX plugin_publication_idx ON plugin_events(publication_id,sequence);
CREATE INDEX plugin_request_idx ON plugin_events(request_id,sequence);
CREATE INDEX plugin_origin_idx ON plugin_events(origin_request_id,sequence);
UPDATE metadata SET value='7' WHERE key='schema_version' AND value='6';
";
fn append<O: PublicationTable>(
    tx: &Connection,
    context: &opencut_editor_application::audit::ExecutionContext,
    id: &str,
    origin: &str,
    intent: &Intent<O>,
    phase: PublicationPhase,
    observed: Option<&str>,
) -> Result<(), EditorError> {
    let prefix = O::TABLE_PREFIX;
    let event = MaintenanceEvent {
        sequence: String::new(),
        publication_id: id.into(),
        phase,
        intent: intent.clone(),
        actor: context.actor().clone(),
        request_id: context.request_id().into(),
        origin_request_id: origin.into(),
        observed_hash: observed.map(str::to_owned),
        created_at: String::new(),
    };
    tx.execute(&format!("INSERT INTO {prefix}_events(publication_id,request_id,origin_request_id,object_json,event_json) VALUES (?1,?2,?3,?4,?5)"),params![id,context.request_id(),origin,encode_json(&intent.object,"maintenance object")?,encode_json(&event,"maintenance event")?]).map_err(storage_error)?;
    Ok(())
}
impl<O: PublicationTable> PublicationJournal<O> for EditorApplication {
    fn prepare(&self, intent: &Intent<O>) -> Result<String, EditorError> {
        let prefix = O::TABLE_PREFIX;
        sqlite_access::owner(&self.connection.1)?;
        maintenance::validate_intent(intent)?;
        let id = format!("publication_{}", Uuid::new_v4().simple());
        let tx =
            rusqlite::Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
                .map_err(storage_error)?;
        tx.execute(&format!("INSERT INTO {prefix}_publications(publication_id,object_json,intent_json,origin_request_id,pending) VALUES (?1,?2,?3,?4,1)"),params![id,encode_json(&intent.object,"maintenance object")?,encode_json(intent,"publication intent")?,self.connection.1.request_id()]).map_err(storage_error)?;
        append(
            &tx,
            &self.connection.1,
            &id,
            self.connection.1.request_id(),
            intent,
            PublicationPhase::Prepared,
            intent.before_hash.as_deref(),
        )?;
        tx.commit().map_err(storage_error)?;
        Ok(id)
    }
    fn complete(&self, id: &str) -> Result<(), EditorError> {
        let prefix = O::TABLE_PREFIX;
        sqlite_access::owner(&self.connection.1)?;
        let tx =
            rusqlite::Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
                .map_err(storage_error)?;
        let row:Option<(String,String)>=tx.query_row(&format!("SELECT intent_json,origin_request_id FROM {prefix}_publications WHERE publication_id=?1 AND pending=1"),[id],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(storage_error)?;
        let (intent, origin) = row.ok_or_else(|| {
            EditorError::new("PUBLICATION_STATE_CONFLICT", "Publication is not pending")
        })?;
        let intent: Intent<O> = decode_json(&intent, "publication intent")?;
        append(
            &tx,
            &self.connection.1,
            id,
            &origin,
            &intent,
            PublicationPhase::Completed,
            Some(&intent.after_hash),
        )?;
        tx.execute(
            &format!("UPDATE {prefix}_publications SET pending=0 WHERE publication_id=?1"),
            [id],
        )
        .map_err(storage_error)?;
        tx.commit().map_err(storage_error)?;
        Ok(())
    }
    fn reconcile(&self, object: &O, observed: Option<&str>) -> Result<Vec<String>, EditorError> {
        let prefix = O::TABLE_PREFIX;
        sqlite_access::owner(&self.connection.1)?;
        maintenance::validate_object(object)?;
        let tx =
            rusqlite::Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
                .map_err(storage_error)?;
        let rows = {
            let mut statement=tx.prepare(&format!("SELECT publication_id,intent_json,origin_request_id FROM {prefix}_publications WHERE object_json=?1 AND pending=1 ORDER BY rowid")).map_err(storage_error)?;
            statement
                .query_map([encode_json(object, "maintenance object")?], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                })
                .map_err(storage_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(storage_error)?
        };
        let mut resolved = vec![];
        for (id, intent, origin) in rows {
            let intent: Intent<O> = decode_json(&intent, "publication intent")?;
            let phase = if observed == Some(intent.after_hash.as_str()) {
                PublicationPhase::ObservedApplied
            } else if observed == intent.before_hash.as_deref() {
                PublicationPhase::ObservedUnchanged
            } else {
                PublicationPhase::ObservedDiverged
            };
            append(
                &tx,
                &self.connection.1,
                &id,
                &origin,
                &intent,
                phase,
                observed,
            )?;
            tx.execute(
                &format!("UPDATE {prefix}_publications SET pending=0 WHERE publication_id=?1"),
                [&id],
            )
            .map_err(storage_error)?;
            resolved.push(id);
        }
        tx.commit().map_err(storage_error)?;
        Ok(resolved)
    }
}
impl<O: PublicationTable> MaintenanceRead<O> for SqliteConnection {
    fn read_maintenance(
        &self,
        target: &MaintenanceTarget<O>,
        after: i64,
        limit: usize,
    ) -> Result<Vec<Event<O>>, EditorError> {
        let prefix = O::TABLE_PREFIX;
        sqlite_access::owner(&self.1)?;
        let (filter, id) = match target {
            MaintenanceTarget::Workspace => ("1=1", String::new()),
            MaintenanceTarget::Object { object } => {
                ("object_json=?2", encode_json(object, "maintenance object")?)
            }
            MaintenanceTarget::Publication { id } => ("publication_id=?2", id.clone()),
            MaintenanceTarget::Request { id } => {
                ("(request_id=?2 OR origin_request_id=?2)", id.clone())
            }
        };
        let mut statement=self.prepare(&format!("SELECT sequence,event_json,created_at FROM {prefix}_events WHERE sequence>?1 AND {filter} AND ?2 IS NOT NULL ORDER BY sequence LIMIT ?3")).map_err(storage_error)?;
        statement
            .query_map(params![after, id, limit as i64], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(storage_error)?
            .map(|r| {
                let (sequence, json, created_at) = r.map_err(storage_error)?;
                let mut event: Event<O> = decode_json(&json, "maintenance event")?;
                event.sequence = sequence.to_string();
                event.created_at = created_at;
                Ok(event)
            })
            .collect()
    }
}
impl EditorApplication {
    pub fn plugin_publication_query(
        &self,
        input: plugin_publication::Query,
    ) -> Result<plugin_publication::Page, EditorError> {
        maintenance::query(&self.connection, self.connection.1.caller(), input)
    }
    pub fn maintenance_query(
        &self,
        input: MaintenanceQuery,
    ) -> Result<MaintenancePage, EditorError> {
        maintenance::query(&self.connection, self.connection.1.caller(), input)
    }
}
