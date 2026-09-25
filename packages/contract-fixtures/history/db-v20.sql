-- Frozen workspace-20 branch and merge-parent references.
CREATE TABLE project_branches (
 project_id TEXT NOT NULL REFERENCES projects(project_id),
 branch_name TEXT NOT NULL CHECK(branch_name <> 'main'),
 head_revision TEXT NOT NULL REFERENCES revisions(revision),
 created_from_revision TEXT NOT NULL REFERENCES revisions(revision),
 created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
 PRIMARY KEY(project_id,branch_name)
);
CREATE INDEX project_branches_head_idx ON project_branches(head_revision);
CREATE TABLE project_merge_parents (
 revision TEXT PRIMARY KEY REFERENCES revisions(revision),
 project_id TEXT NOT NULL REFERENCES projects(project_id),
 source_revision TEXT NOT NULL REFERENCES revisions(revision)
);
CREATE INDEX project_merge_source_idx ON project_merge_parents(project_id,source_revision);
UPDATE metadata SET value='20' WHERE key='schema_version' AND value='19';
