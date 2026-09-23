-- Frozen historical format fixture. Apply v1 then successive deltas.
ALTER TABLE jobs ADD COLUMN progress_json TEXT;
                     CREATE TABLE IF NOT EXISTS job_events (
                       sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                       event_id TEXT NOT NULL UNIQUE,
                       job_id TEXT NOT NULL REFERENCES jobs(job_id),
                       event_type TEXT NOT NULL,
                       state TEXT NOT NULL,
                       attempt INTEGER NOT NULL,
                       payload_json TEXT NOT NULL,
                       created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                     );
                     CREATE INDEX IF NOT EXISTS job_events_job_idx ON job_events(job_id, sequence);
                     UPDATE metadata SET value = '3' WHERE key = 'schema_version' AND value = '2';
