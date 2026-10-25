//! Historical storage DDL and the one current recovery table.

pub(super) const CREATE_TIMELINE_STORE: &str = "
CREATE TABLE edit_recovery_v1(
 recovery_id TEXT PRIMARY KEY,
 project_id TEXT NOT NULL REFERENCES projects(project_id),
 principal_id TEXT NOT NULL,
 credential_id TEXT NOT NULL,
 owner INTEGER NOT NULL CHECK(owner IN (0,1)),
 request_digest TEXT NOT NULL,
 context_json TEXT NOT NULL,
 stored_bytes INTEGER NOT NULL CHECK(stored_bytes > 0 AND stored_bytes <= 2097152),
 UNIQUE(project_id,principal_id,credential_id,owner)
);
UPDATE metadata SET value='28' WHERE key='schema_version' AND value='27';
";

pub(super) const CREATE_INTENT_STORE: &str = "
CREATE TABLE edit_recovery_v2(
 recovery_id TEXT PRIMARY KEY,
 project_id TEXT NOT NULL REFERENCES projects(project_id),
 principal_id TEXT NOT NULL,
 credential_id TEXT NOT NULL,
 owner INTEGER NOT NULL CHECK(owner IN (0,1)),
 request_digest TEXT NOT NULL,
 context_json TEXT NOT NULL,
 stored_bytes INTEGER NOT NULL CHECK(stored_bytes > 0 AND stored_bytes <= 2097152),
 UNIQUE(project_id,principal_id,credential_id,owner)
);
CREATE TRIGGER recovery_v1_excludes_v2 BEFORE INSERT ON edit_recovery_v1
WHEN EXISTS(SELECT 1 FROM edit_recovery_v2 WHERE project_id=NEW.project_id AND principal_id=NEW.principal_id AND credential_id=NEW.credential_id AND owner=NEW.owner)
BEGIN SELECT RAISE(ABORT,'unresolved recovery scope conflict'); END;
CREATE TRIGGER recovery_v2_excludes_v1 BEFORE INSERT ON edit_recovery_v2
WHEN EXISTS(SELECT 1 FROM edit_recovery_v1 WHERE project_id=NEW.project_id AND principal_id=NEW.principal_id AND credential_id=NEW.credential_id AND owner=NEW.owner)
BEGIN SELECT RAISE(ABORT,'unresolved recovery scope conflict'); END;
UPDATE metadata SET value='29' WHERE key='schema_version' AND value='28';
";

pub(super) const CONSOLIDATE: &str = "
CREATE TABLE edit_recovery(
 recovery_id TEXT PRIMARY KEY,
 project_id TEXT NOT NULL REFERENCES projects(project_id),
 principal_id TEXT NOT NULL,
 credential_id TEXT NOT NULL,
 owner INTEGER NOT NULL CHECK(owner IN (0,1)),
 request_digest TEXT NOT NULL,
 context_json TEXT NOT NULL,
 stored_bytes INTEGER NOT NULL CHECK(stored_bytes > 0 AND stored_bytes <= 2097152),
 UNIQUE(project_id,principal_id,credential_id,owner)
);
INSERT INTO edit_recovery SELECT recovery_id,project_id,principal_id,credential_id,owner,request_digest,context_json,stored_bytes FROM edit_recovery_v1;
INSERT INTO edit_recovery SELECT recovery_id,project_id,principal_id,credential_id,owner,request_digest,context_json,stored_bytes FROM edit_recovery_v2;
DROP TRIGGER recovery_v1_excludes_v2;
DROP TRIGGER recovery_v2_excludes_v1;
DROP TABLE edit_recovery_v1;
DROP TABLE edit_recovery_v2;
DROP TRIGGER font_derivations_v1_immutable_update;
DROP TRIGGER font_derivations_v1_immutable_delete;
ALTER TABLE font_derivations_v1 RENAME TO font_derivations;
CREATE TRIGGER font_derivations_immutable_update BEFORE UPDATE ON font_derivations
BEGIN SELECT RAISE(ABORT,'immutable font derivation'); END;
CREATE TRIGGER font_derivations_immutable_delete BEFORE DELETE ON font_derivations
BEGIN SELECT RAISE(ABORT,'retained font derivation'); END;
UPDATE metadata SET value='33' WHERE key='schema_version' AND value='32';
";
