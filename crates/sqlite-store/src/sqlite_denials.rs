//! Rejected operations have already rolled back before this independent append.
use super::*;
use opencut_editor_application::denials::*;
pub(super) const MIGRATION:&str="
CREATE TABLE denial_records(
 sequence INTEGER PRIMARY KEY AUTOINCREMENT,request_id TEXT NOT NULL UNIQUE,
 source_json TEXT NOT NULL,principal_id TEXT,credential_id TEXT,
 operation_json TEXT NOT NULL,reason_json TEXT NOT NULL,created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
CREATE INDEX denial_principal_idx ON denial_records(principal_id,sequence);
CREATE INDEX denial_credential_idx ON denial_records(credential_id,sequence);
UPDATE metadata SET value='11' WHERE key='schema_version' AND value='10';
";
impl DenialStore for EditorApplication {
    fn record_denial(&self, denial: &Denial<'_>) -> Result<(), EditorError> {
        validate(denial)?;
        let principal = denial.caller.map(|c| c.principal_id());
        let credential = denial.caller.and_then(|c| match c {
            access::Caller::Credential(c) => Some(c.credential_id()),
            _ => None,
        });
        let tx =
            rusqlite::Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
                .map_err(storage_error)?;
        let exists: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM denial_records WHERE request_id=?1)",
                [denial.request_id],
                |r| r.get(0),
            )
            .map_err(storage_error)?;
        if exists {
            return tx.commit().map_err(storage_error);
        }
        tx.execute("INSERT INTO denial_records(request_id,source_json,principal_id,credential_id,operation_json,reason_json) VALUES (?1,?2,?3,?4,?5,?6) ON CONFLICT(request_id) DO NOTHING",params![denial.request_id,encode_json(&denial.source,"denial source")?,principal,credential,encode_json(&denial.operation,"denial action")?,encode_json(&denial.reason,"denial reason")?]).map_err(storage_error)?;
        tx.execute("DELETE FROM denial_records WHERE sequence <= (SELECT sequence FROM denial_records ORDER BY sequence DESC LIMIT 1 OFFSET ?1)",[MAX_RETAINED as i64]).map_err(storage_error)?;
        tx.commit().map_err(storage_error)
    }
    fn read_denials(&self, query: &DenialQuery) -> Result<DenialPage, EditorError> {
        sqlite_access::owner(&self.connection.1)?;
        let after = validate_query(query)?;
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let oldest: Option<i64> = tx
            .query_row("SELECT MIN(sequence) FROM denial_records", [], |r| r.get(0))
            .map_err(storage_error)?;
        if after > 0 && oldest.is_some_and(|first| after < first - 1) {
            return Err(EditorError::new(
                "CURSOR_EXPIRED",
                "Denial audit retention advanced; restart discovery at sequence 0",
            ));
        }
        let (filter, id) = match &query.target {
            DenialTarget::Workspace => ("1=1", ""),
            DenialTarget::Request(id) => ("request_id=?2", id.as_str()),
            DenialTarget::Principal(id) => ("principal_id=?2", id.as_str()),
            DenialTarget::Credential(id) => ("credential_id=?2", id.as_str()),
        };
        let mut statement=tx.prepare(&format!("SELECT sequence,request_id,source_json,principal_id,credential_id,operation_json,reason_json,created_at FROM denial_records WHERE sequence>?1 AND {filter} AND ?2 IS NOT NULL ORDER BY sequence LIMIT ?3")).map_err(storage_error)?;
        let mut rows = statement
            .query(params![after, id, (query.limit + 1) as i64])
            .map_err(storage_error)?;
        let mut result = DenialPage {
            records: vec![],
            last_sequence: query.after_sequence.clone(),
            has_more: false,
            oldest_available_sequence: oldest.map(|s| s.to_string()),
            max_retained_records: MAX_RETAINED,
            coverage: "bounded_host_denials_since_schema11".into(),
        };
        while let Some(r) = rows.next().map_err(storage_error)? {
            if result.records.len() == query.limit {
                result.has_more = true;
                break;
            }
            let sequence = r.get::<_, i64>(0).map_err(storage_error)?.to_string();
            result.last_sequence = sequence.clone();
            result.records.push(DenialRecord {
                sequence,
                request_id: r.get(1).map_err(storage_error)?,
                source: decode_json(
                    &r.get::<_, String>(2).map_err(storage_error)?,
                    "denial source",
                )?,
                principal_id: r.get(3).map_err(storage_error)?,
                credential_id: r.get(4).map_err(storage_error)?,
                operation: decode_json(
                    &r.get::<_, String>(5).map_err(storage_error)?,
                    "denial action",
                )?,
                reason: decode_json(
                    &r.get::<_, String>(6).map_err(storage_error)?,
                    "denial reason",
                )?,
                created_at: r.get(7).map_err(storage_error)?,
            });
        }
        Ok(result)
    }
}
