-- Frozen historical format fixture. Apply v1 then successive deltas.
CREATE TABLE metadata (
                   key TEXT PRIMARY KEY,
                   value TEXT NOT NULL
                 );
                 CREATE TABLE projects (
                   project_id TEXT PRIMARY KEY,
                   name TEXT NOT NULL,
                   head_revision TEXT NOT NULL
                 );
                 CREATE TABLE revisions (
                   revision TEXT PRIMARY KEY,
                   project_id TEXT NOT NULL REFERENCES projects(project_id),
                   parent_revision TEXT,
                   transaction_id TEXT,
                   document_json TEXT NOT NULL,
                   created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                 );
                 CREATE INDEX revisions_project_idx
                   ON revisions(project_id, created_at);
                 CREATE TABLE editor_transactions (
                   transaction_id TEXT PRIMARY KEY,
                   project_id TEXT NOT NULL REFERENCES projects(project_id),
                   base_revision TEXT NOT NULL,
                   revision TEXT NOT NULL REFERENCES revisions(revision),
                   request_json TEXT NOT NULL,
                   result_json TEXT NOT NULL,
                   created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                 );
                 CREATE TABLE idempotency (
                   operation_family TEXT NOT NULL,
                   scope TEXT NOT NULL,
                   idempotency_key TEXT NOT NULL,
                   request_hash TEXT NOT NULL,
                   result_kind TEXT NOT NULL,
                   result_json TEXT NOT NULL,
                   created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                   PRIMARY KEY(operation_family, scope, idempotency_key)
                 );
                 CREATE TABLE outbox (
                   sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                   event_id TEXT NOT NULL UNIQUE,
                   project_id TEXT NOT NULL REFERENCES projects(project_id),
                   revision TEXT NOT NULL REFERENCES revisions(revision),
                   event_type TEXT NOT NULL,
                   payload_json TEXT NOT NULL,
                   created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                 );
                 INSERT INTO metadata(key, value) VALUES ('schema_version', '1');
