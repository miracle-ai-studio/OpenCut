-- Frozen workspace-21 persistent edit-control state.
CREATE TABLE project_edit_control (
 project_id TEXT PRIMARY KEY REFERENCES projects(project_id),
 generation INTEGER NOT NULL CHECK(generation >= 1),
 held INTEGER NOT NULL CHECK(held IN (0,1)),
 held_by_principal TEXT
);
UPDATE metadata SET value='21' WHERE key='schema_version' AND value='20';
