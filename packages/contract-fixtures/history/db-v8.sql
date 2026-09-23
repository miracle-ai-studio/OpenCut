-- Frozen schema-8 upload and project-media storage contract.
-- Upload acknowledgement is separate from immutable project revisions.
CREATE TABLE upload_sessions (
 upload_id TEXT PRIMARY KEY,
 project_id TEXT NOT NULL REFERENCES projects(project_id),
 idempotency_key TEXT NOT NULL,
 request_hash TEXT NOT NULL,
 name TEXT NOT NULL,
 size_bytes INTEGER NOT NULL,
 content_hash TEXT NOT NULL,
 next_offset INTEGER NOT NULL DEFAULT 0,
 state TEXT NOT NULL DEFAULT 'receiving',
 job_id TEXT REFERENCES jobs(job_id),
 created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
 UNIQUE(project_id,idempotency_key)
);
CREATE INDEX upload_project_state_idx ON upload_sessions(project_id,state);
CREATE TABLE upload_chunks (
 upload_id TEXT NOT NULL REFERENCES upload_sessions(upload_id),
 offset_bytes INTEGER NOT NULL,
 size_bytes INTEGER NOT NULL,
 content_hash TEXT NOT NULL,
 request_id TEXT NOT NULL,
 PRIMARY KEY(upload_id,offset_bytes)
);
CREATE TABLE project_media (
 media_id TEXT PRIMARY KEY,
 project_id TEXT NOT NULL REFERENCES projects(project_id),
 upload_id TEXT NOT NULL UNIQUE REFERENCES upload_sessions(upload_id),
 job_id TEXT NOT NULL REFERENCES jobs(job_id),
 asset_json TEXT NOT NULL,
 created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX project_media_project_idx ON project_media(project_id,media_id);
UPDATE metadata SET value='8' WHERE key='schema_version' AND value='7';
