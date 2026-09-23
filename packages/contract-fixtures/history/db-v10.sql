-- Frozen schema-10 project-scoped plugin grants. No grant is inferred on upgrade.
CREATE TABLE project_plugin_grants(
 project_id TEXT NOT NULL REFERENCES projects(project_id),
 plugin_id TEXT NOT NULL, version TEXT NOT NULL,
 revision INTEGER NOT NULL CHECK(revision>0), grant_json TEXT NOT NULL,
 PRIMARY KEY(project_id,plugin_id,version));
UPDATE metadata SET value='10' WHERE key='schema_version' AND value='9';
