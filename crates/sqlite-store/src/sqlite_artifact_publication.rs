//! Durable publication fence for artifact removal. Every persisted authority
//! inspected by the collector must be unable to add a reference while a
//! removal intent is pending, including after its process has crashed.

pub(super) const MIGRATION: &str = "
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
";

#[cfg(test)]
pub(super) const DROP_FOR_TEST: &str = "
DROP TRIGGER artifact_gc_revisions_insert;
DROP TRIGGER artifact_gc_revisions_update;
DROP TRIGGER artifact_gc_jobs_insert;
DROP TRIGGER artifact_gc_jobs_update;
DROP TRIGGER artifact_gc_project_media_insert;
DROP TRIGGER artifact_gc_project_media_update;
DROP TRIGGER artifact_gc_plugin_publications_insert;
DROP TRIGGER artifact_gc_plugin_publications_update;
DROP TRIGGER artifact_gc_project_plugin_grants_insert;
DROP TRIGGER artifact_gc_project_plugin_grants_update;
DROP TABLE artifact_gc_removals;
";

#[cfg(test)]
mod tests {
    use super::super::*;

    #[test]
    fn persisted_removal_fences_every_reference_authority_across_connections() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("workspace.sqlite");
        let mut owner = EditorApplication::open(&database).unwrap();
        let other = Connection::open(&database).unwrap();
        other.busy_timeout(Duration::from_millis(30)).unwrap();
        owner
            .connection
            .execute(
                "INSERT INTO projects(project_id,name,head_revision) VALUES ('p','P','r')",
                [],
            )
            .unwrap();
        owner.connection.execute("INSERT INTO jobs(job_id,state,attempt,kind,input_json,cancel_requested) VALUES ('producer','queued',0,'render.export','{}',0)",[]).unwrap();
        owner.connection.execute("INSERT INTO revisions(revision,project_id,document_json) VALUES ('prior','p','{}')",[]).unwrap();
        owner.connection.execute("INSERT INTO upload_sessions(upload_id,project_id,idempotency_key,request_hash,name,size_bytes,content_hash,next_offset) VALUES ('u','p','k','h','source',1,'h',1)",[]).unwrap();
        owner.connection.execute("INSERT INTO project_media(media_id,project_id,upload_id,job_id,asset_json) VALUES ('prior-media','p','u','producer','{}')",[]).unwrap();
        owner.connection.execute("INSERT INTO plugin_publications(publication_id,object_json,intent_json,origin_request_id,pending) VALUES ('prior-pub','{}','{}','req',0)",[]).unwrap();
        owner.connection.execute("INSERT INTO project_plugin_grants(project_id,plugin_id,version,revision,grant_json) VALUES ('p','plugin','1',1,'{}')",[]).unwrap();
        owner.connection.execute("INSERT INTO artifact_gc_plans(plan_id,idempotency_key,input_json,plan_json) VALUES ('agc_test','test','{}','{}')",[]).unwrap();

        let held = owner
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let locked = other.execute("INSERT INTO jobs(job_id,state,attempt,kind,input_json,cancel_requested) VALUES ('before','queued',0,'render.export','{}',0)",[]).unwrap_err();
        assert_eq!(locked.sqlite_error_code(), Some(ErrorCode::DatabaseBusy));
        held.execute(
            "INSERT INTO artifact_gc_removals(plan_id,state) VALUES ('agc_test','removing')",
            [],
        )
        .unwrap();
        held.commit().unwrap();

        for sql in [
            "INSERT INTO revisions(revision,project_id,document_json) VALUES ('r','p','{}')",
            "UPDATE revisions SET document_json='{\"source\":\"new\"}' WHERE revision='prior'",
            "INSERT INTO jobs(job_id,state,attempt,kind,input_json,cancel_requested) VALUES ('consumer','queued',0,'render.export','{}',0)",
            "UPDATE jobs SET input_json='{\"source\":\"new\"}' WHERE job_id='producer'",
            "UPDATE jobs SET result_json='{}' WHERE job_id='producer'",
            "INSERT INTO project_media(media_id,project_id,upload_id,job_id,asset_json) VALUES ('m','p','u','producer','{}')",
            "UPDATE project_media SET asset_json='{\"source\":\"new\"}' WHERE media_id='prior-media'",
            "INSERT INTO plugin_publications(publication_id,object_json,intent_json,origin_request_id,pending) VALUES ('pub','{}','{}','req',0)",
            "UPDATE plugin_publications SET object_json='{\"source\":\"new\"}' WHERE publication_id='prior-pub'",
            "UPDATE plugin_publications SET intent_json='{\"source\":\"new\"}' WHERE publication_id='prior-pub'",
            "INSERT INTO project_plugin_grants(project_id,plugin_id,version,revision,grant_json) VALUES ('p','plugin','2',1,'{}')",
            "UPDATE project_plugin_grants SET grant_json='{\"source\":\"new\"}' WHERE project_id='p' AND plugin_id='plugin' AND version='1'",
        ] {
            let error = other.execute(sql, []).unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("artifact collection publication fenced"),
                "{sql}: {error}"
            );
        }
        // Worker progress and other non-reference metadata may continue.
        assert_eq!(
            other
                .execute(
                    "UPDATE jobs SET state='running' WHERE job_id='producer'",
                    []
                )
                .unwrap(),
            1
        );
        assert_eq!(
            other
                .query_row("SELECT count(*) FROM revisions", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        other
            .execute(
                "UPDATE artifact_gc_removals SET state='changed' WHERE plan_id='agc_test'",
                [],
            )
            .unwrap();
        other
            .execute(
                "INSERT INTO revisions(revision,project_id,document_json) VALUES ('r','p','{}')",
                [],
            )
            .unwrap();
        other.execute("INSERT INTO jobs(job_id,state,attempt,kind,input_json,cancel_requested) VALUES ('consumer','queued',0,'render.export','{}',0)",[]).unwrap();
    }
}
