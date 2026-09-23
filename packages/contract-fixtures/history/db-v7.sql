-- Frozen independently specified schema-7 plugin-publication extension.
-- Plugin and media histories have distinct sequence spaces and wire types.
CREATE TABLE plugin_publications (
 publication_id TEXT PRIMARY KEY, object_json TEXT NOT NULL, intent_json TEXT NOT NULL,
 origin_request_id TEXT NOT NULL, pending INTEGER NOT NULL
);
CREATE INDEX plugin_pending_idx ON plugin_publications(object_json,pending);
CREATE TABLE plugin_events (
 sequence INTEGER PRIMARY KEY AUTOINCREMENT,
 publication_id TEXT NOT NULL REFERENCES plugin_publications(publication_id),
 request_id TEXT NOT NULL, origin_request_id TEXT NOT NULL,
 object_json TEXT NOT NULL, event_json TEXT NOT NULL,
 created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX plugin_object_idx ON plugin_events(object_json,sequence);
CREATE INDEX plugin_publication_idx ON plugin_events(publication_id,sequence);
CREATE INDEX plugin_request_idx ON plugin_events(request_id,sequence);
CREATE INDEX plugin_origin_idx ON plugin_events(origin_request_id,sequence);
UPDATE metadata SET value='7' WHERE key='schema_version' AND value='6';
