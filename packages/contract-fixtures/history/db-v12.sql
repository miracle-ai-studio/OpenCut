-- Frozen schema-12 reviewed CAS collection plans and step receipts.
CREATE TABLE cas_gc_plans(plan_id TEXT PRIMARY KEY,idempotency_key TEXT NOT NULL UNIQUE,input_json TEXT NOT NULL,plan_json TEXT NOT NULL,created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
CREATE TABLE cas_gc_events(sequence INTEGER PRIMARY KEY AUTOINCREMENT,plan_id TEXT NOT NULL REFERENCES cas_gc_plans(plan_id),item_index INTEGER NOT NULL,state_json TEXT NOT NULL,request_id TEXT NOT NULL,actor_json TEXT NOT NULL,created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
CREATE INDEX cas_gc_events_plan_idx ON cas_gc_events(plan_id,sequence);
UPDATE metadata SET value='12' WHERE key='schema_version' AND value='11';
