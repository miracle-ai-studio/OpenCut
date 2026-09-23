-- Frozen schema-6 native-publication extension; previous fixtures remain unchanged.
CREATE TABLE maintenance_publications (
 publication_id TEXT PRIMARY KEY, object_json TEXT NOT NULL, intent_json TEXT NOT NULL,
 origin_request_id TEXT NOT NULL, pending INTEGER NOT NULL
);
CREATE INDEX maintenance_pending_idx ON maintenance_publications(object_json,pending);
CREATE TABLE maintenance_events (
 sequence INTEGER PRIMARY KEY AUTOINCREMENT,
 publication_id TEXT NOT NULL REFERENCES maintenance_publications(publication_id),
 request_id TEXT NOT NULL, origin_request_id TEXT NOT NULL,
 object_json TEXT NOT NULL, event_json TEXT NOT NULL,
 created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX maintenance_object_idx ON maintenance_events(object_json,sequence);
CREATE INDEX maintenance_publication_idx ON maintenance_events(publication_id,sequence);
CREATE INDEX maintenance_request_idx ON maintenance_events(request_id,sequence);
CREATE INDEX maintenance_origin_idx ON maintenance_events(origin_request_id,sequence);
UPDATE metadata SET value='6' WHERE key='schema_version' AND value='5';
