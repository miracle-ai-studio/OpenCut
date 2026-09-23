-- Frozen schema-9 logical upload staging reservations.
-- Existing sessions conservatively retain their declared size until native cleanup.
CREATE TABLE upload_retention (
 upload_id TEXT PRIMARY KEY REFERENCES upload_sessions(upload_id),
 reserved_bytes INTEGER NOT NULL CHECK(reserved_bytes>0),
 released INTEGER NOT NULL DEFAULT 0 CHECK(released IN (0,1)),
 released_at TEXT
);
CREATE INDEX upload_retention_live_idx ON upload_retention(released,upload_id);
INSERT INTO upload_retention(upload_id,reserved_bytes)
 SELECT upload_id,size_bytes FROM upload_sessions;
UPDATE metadata SET value='9' WHERE key='schema_version' AND value='8';
