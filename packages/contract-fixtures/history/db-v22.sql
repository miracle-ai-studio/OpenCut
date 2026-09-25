-- Frozen workspace-22 project-scoped Plugin V2 preset grants.
CREATE TABLE project_preset_grants(
 project_id TEXT NOT NULL REFERENCES projects(project_id),
 plugin_id TEXT NOT NULL, version TEXT NOT NULL,
 revision INTEGER NOT NULL CHECK(revision>0), grant_json TEXT NOT NULL,
 PRIMARY KEY(project_id,plugin_id,version));
UPDATE metadata SET value='22' WHERE key='schema_version' AND value='21';
