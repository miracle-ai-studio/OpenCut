-- Frozen schema-4 provenance extension. Earlier fixtures remain unchanged.
CREATE TABLE audit_records (
 sequence INTEGER PRIMARY KEY AUTOINCREMENT,
 audit_id TEXT NOT NULL UNIQUE,
 request_id TEXT NOT NULL,
 origin_request_id TEXT,
 actor_json TEXT NOT NULL,
 action TEXT NOT NULL,
 project_id TEXT REFERENCES projects(project_id),
 revision TEXT REFERENCES revisions(revision),
 transaction_id TEXT REFERENCES editor_transactions(transaction_id),
 job_id TEXT REFERENCES jobs(job_id),
 attempt INTEGER,
 artifact_ids_json TEXT NOT NULL,
 created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX audit_project_idx ON audit_records(project_id, sequence);
CREATE INDEX audit_job_idx ON audit_records(job_id, sequence);
CREATE INDEX audit_transaction_idx ON audit_records(transaction_id, sequence);
CREATE INDEX audit_request_idx ON audit_records(request_id, sequence);
CREATE INDEX audit_origin_idx ON audit_records(origin_request_id, sequence);
UPDATE metadata SET value='4' WHERE key='schema_version' AND value='3';
