-- Frozen schema-5 access extension. Historical fixtures v1-v4 remain immutable.
CREATE TABLE access_principals (
 principal_id TEXT PRIMARY KEY, revision INTEGER NOT NULL, enabled INTEGER NOT NULL
);
CREATE TABLE access_grants (
 principal_id TEXT NOT NULL REFERENCES access_principals(principal_id),
 project_id TEXT NOT NULL REFERENCES projects(project_id), action_mask INTEGER NOT NULL,
 PRIMARY KEY(principal_id, project_id)
);
CREATE TABLE access_credentials (
 credential_id TEXT PRIMARY KEY, principal_id TEXT NOT NULL REFERENCES access_principals(principal_id),
 secret_digest TEXT NOT NULL UNIQUE, revoked INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE access_events (
 sequence INTEGER PRIMARY KEY AUTOINCREMENT, principal_id TEXT NOT NULL, credential_id TEXT,
 action TEXT NOT NULL, request_id TEXT NOT NULL, actor_json TEXT NOT NULL, policy_json TEXT,
 created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE job_projects (
 job_id TEXT PRIMARY KEY REFERENCES jobs(job_id), project_id TEXT NOT NULL REFERENCES projects(project_id),
 revision TEXT NOT NULL REFERENCES revisions(revision)
);
CREATE INDEX job_projects_project_idx ON job_projects(project_id,job_id);
UPDATE metadata SET value='5' WHERE key='schema_version' AND value='4';
