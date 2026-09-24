-- Frozen workspace-16 artifact retention release records.
CREATE TABLE artifact_retention(
  artifact_id TEXT PRIMARY KEY,
  job_id TEXT NOT NULL REFERENCES jobs(job_id),
  idempotency_key TEXT NOT NULL UNIQUE,
  released_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  request_id TEXT NOT NULL,
  actor_json TEXT NOT NULL
);
CREATE INDEX artifact_retention_job_idx ON artifact_retention(job_id);
UPDATE metadata SET value='16' WHERE key='schema_version' AND value='15';
