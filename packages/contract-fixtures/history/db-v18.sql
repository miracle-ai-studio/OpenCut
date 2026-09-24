-- Frozen workspace-18 durable artifact publication fence.

CREATE TABLE artifact_gc_removals(
  plan_id TEXT PRIMARY KEY REFERENCES artifact_gc_plans(plan_id),
  state TEXT NOT NULL CHECK(state IN ('removing','removed','absent','changed')),
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TRIGGER artifact_gc_revisions_insert BEFORE INSERT ON revisions
WHEN EXISTS(SELECT 1 FROM artifact_gc_removals WHERE state='removing')
BEGIN SELECT RAISE(ABORT,'artifact collection publication fenced'); END;
CREATE TRIGGER artifact_gc_revisions_update BEFORE UPDATE OF document_json ON revisions
WHEN EXISTS(SELECT 1 FROM artifact_gc_removals WHERE state='removing')
BEGIN SELECT RAISE(ABORT,'artifact collection publication fenced'); END;
CREATE TRIGGER artifact_gc_jobs_insert BEFORE INSERT ON jobs
WHEN EXISTS(SELECT 1 FROM artifact_gc_removals WHERE state='removing')
BEGIN SELECT RAISE(ABORT,'artifact collection publication fenced'); END;
CREATE TRIGGER artifact_gc_jobs_update BEFORE UPDATE OF input_json,result_json ON jobs
WHEN EXISTS(SELECT 1 FROM artifact_gc_removals WHERE state='removing')
BEGIN SELECT RAISE(ABORT,'artifact collection publication fenced'); END;
CREATE TRIGGER artifact_gc_project_media_insert BEFORE INSERT ON project_media
WHEN EXISTS(SELECT 1 FROM artifact_gc_removals WHERE state='removing')
BEGIN SELECT RAISE(ABORT,'artifact collection publication fenced'); END;
CREATE TRIGGER artifact_gc_project_media_update BEFORE UPDATE OF asset_json ON project_media
WHEN EXISTS(SELECT 1 FROM artifact_gc_removals WHERE state='removing')
BEGIN SELECT RAISE(ABORT,'artifact collection publication fenced'); END;
CREATE TRIGGER artifact_gc_plugin_publications_insert BEFORE INSERT ON plugin_publications
WHEN EXISTS(SELECT 1 FROM artifact_gc_removals WHERE state='removing')
BEGIN SELECT RAISE(ABORT,'artifact collection publication fenced'); END;
CREATE TRIGGER artifact_gc_plugin_publications_update BEFORE UPDATE OF object_json,intent_json ON plugin_publications
WHEN EXISTS(SELECT 1 FROM artifact_gc_removals WHERE state='removing')
BEGIN SELECT RAISE(ABORT,'artifact collection publication fenced'); END;
CREATE TRIGGER artifact_gc_project_plugin_grants_insert BEFORE INSERT ON project_plugin_grants
WHEN EXISTS(SELECT 1 FROM artifact_gc_removals WHERE state='removing')
BEGIN SELECT RAISE(ABORT,'artifact collection publication fenced'); END;
CREATE TRIGGER artifact_gc_project_plugin_grants_update BEFORE UPDATE OF grant_json ON project_plugin_grants
WHEN EXISTS(SELECT 1 FROM artifact_gc_removals WHERE state='removing')
BEGIN SELECT RAISE(ABORT,'artifact collection publication fenced'); END;
UPDATE metadata SET value='18' WHERE key='schema_version' AND value='17';
