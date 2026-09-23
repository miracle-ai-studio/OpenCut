-- Frozen schema-11 bounded host denial history.

CREATE TABLE denial_records(
 sequence INTEGER PRIMARY KEY AUTOINCREMENT,request_id TEXT NOT NULL UNIQUE,
 source_json TEXT NOT NULL,principal_id TEXT,credential_id TEXT,
 operation_json TEXT NOT NULL,reason_json TEXT NOT NULL,created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
CREATE INDEX denial_principal_idx ON denial_records(principal_id,sequence);
CREATE INDEX denial_credential_idx ON denial_records(credential_id,sequence);
UPDATE metadata SET value='11' WHERE key='schema_version' AND value='10';
