-- Frozen historical format fixture. Apply v1 then successive deltas.
CREATE TABLE jobs (
                   job_id TEXT PRIMARY KEY,
                   state TEXT NOT NULL,
                   attempt INTEGER NOT NULL,
                   kind TEXT NOT NULL,
                   input_json TEXT NOT NULL,
                   result_json TEXT,
                   error_json TEXT,
                   lease_owner TEXT,
                   lease_until INTEGER,
                   cancel_requested INTEGER NOT NULL DEFAULT 0,
                   created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                 );
                 CREATE INDEX jobs_claim_idx
                   ON jobs(state, cancel_requested, created_at);
                 UPDATE metadata SET value = '2'
                   WHERE key = 'schema_version' AND value = '1';
