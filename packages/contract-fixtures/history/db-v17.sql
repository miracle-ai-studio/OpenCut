-- Frozen workspace-17 durable artifact review plans.
CREATE TABLE artifact_gc_plans(
  plan_id TEXT PRIMARY KEY,
  idempotency_key TEXT NOT NULL UNIQUE,
  input_json TEXT NOT NULL,
  plan_json TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
UPDATE metadata SET value='17' WHERE key='schema_version' AND value='16';
