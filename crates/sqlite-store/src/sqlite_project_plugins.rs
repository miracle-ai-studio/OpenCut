//! Project plugin policy is independent of the timeline head and global installation.
use super::*;
use opencut_editor_application::project_plugins::*;
pub(super) const MIGRATION: &str = "
CREATE TABLE project_plugin_grants(
 project_id TEXT NOT NULL REFERENCES projects(project_id),
 plugin_id TEXT NOT NULL, version TEXT NOT NULL,
 revision INTEGER NOT NULL CHECK(revision>0), grant_json TEXT NOT NULL,
 PRIMARY KEY(project_id,plugin_id,version));
UPDATE metadata SET value='10' WHERE key='schema_version' AND value='9';
";
fn read(c: &Connection, target: &PluginTarget) -> Result<Option<Grant>, EditorError> {
    let value: Option<String> = c.query_row("SELECT grant_json FROM project_plugin_grants WHERE project_id=?1 AND plugin_id=?2 AND version=?3",params![target.project_id,target.id,target.version],|r|r.get(0)).optional().map_err(storage_error)?;
    value
        .map(|v| decode_json(&v, "project plugin grant"))
        .transpose()
}
impl ProjectPluginStore for EditorApplication {
    fn set_plugin_grant(&mut self, input: &SetGrant) -> Result<Grant, EditorError> {
        sqlite_access::owner(&self.connection.1)?;
        validate_grant(input)?;
        let context = self.connection.1.clone();
        let scope = encode_json(&input.target, "plugin target")?;
        let identity = request_hash("project_plugin_grant", &scope, input)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        sqlite_access::owner(&context)?;
        if let Some(result) = load_idempotent(
            &tx,
            "project_plugin_grant",
            &scope,
            &input.idempotency_key,
            &identity,
            "plugin_grant",
        )? {
            return decode_json(&result, "plugin grant receipt");
        }
        let head_revision =
            sqlite_project_v3::project_head_revision(&tx, &input.target.project_id)?;
        let previous = read(&tx, &input.target)?;
        let revision = previous.as_ref().map_or("0", |g| g.revision.as_str());
        if revision != input.expected_revision {
            return Err(EditorError::new(
                "REVISION_CONFLICT",
                "Project plugin grant revision changed",
            ));
        }
        if previous.is_none() {
            let count: usize = tx
                .query_row(
                    "SELECT COUNT(*) FROM project_plugin_grants WHERE project_id=?1",
                    [&input.target.project_id],
                    |r| r.get(0),
                )
                .map_err(storage_error)?;
            if count >= MAX_GRANTS {
                return Err(EditorError::new(
                    "CAPABILITY_UNAVAILABLE",
                    "Project plugin grant capacity reached",
                ));
            }
        }
        let next = input
            .expected_revision
            .parse::<i64>()
            .map_err(|_| EditorError::new("INVALID_ARGUMENT", "Invalid grant revision"))?
            + 1;
        let grant = Grant {
            target: input.target.clone(),
            package_sha256: input.package_sha256.clone(),
            operations: input.operations.clone(),
            enabled: input.enabled,
            revision: next.to_string(),
        };
        let result = encode_json(&grant, "plugin grant")?;
        tx.execute("INSERT INTO project_plugin_grants(project_id,plugin_id,version,revision,grant_json) VALUES (?1,?2,?3,?4,?5) ON CONFLICT(project_id,plugin_id,version) DO UPDATE SET revision=excluded.revision,grant_json=excluded.grant_json",params![input.target.project_id,input.target.id,input.target.version,next,result]).map_err(storage_error)?;
        tx.execute("INSERT INTO idempotency(operation_family,scope,idempotency_key,request_hash,result_kind,result_json) VALUES ('project_plugin_grant',?1,?2,?3,'plugin_grant',?4)",params![scope,input.idempotency_key,identity.current_digest(),result]).map_err(idempotency_insert_error)?;
        tx.execute("INSERT INTO outbox(event_id,project_id,revision,event_type,payload_json) VALUES (?1,?2,?3,'project.plugin_grant.updated',?4)",params![format!("evt_{}",Uuid::new_v4().simple()),input.target.project_id,head_revision,result]).map_err(storage_error)?;
        sqlite_audit::insert(
            &tx,
            &context,
            "project.plugin_grant.updated",
            sqlite_audit::Links {
                project_id: Some(&input.target.project_id),
                revision: Some(&head_revision),
                ..Default::default()
            },
        )?;
        tx.commit().map_err(storage_error)?;
        Ok(grant)
    }
    fn plugin_grants(&self, project: &str) -> Result<Vec<Grant>, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        sqlite_access::require(&tx, &self.connection.1, project, ProjectAction::Read)?;
        sqlite_project_v3::project_head_revision(&tx, project)?;
        let mut statement=tx.prepare("SELECT grant_json FROM project_plugin_grants WHERE project_id=?1 ORDER BY plugin_id,version LIMIT 64").map_err(storage_error)?;
        statement
            .query_map([project], |r| r.get::<_, String>(0))
            .map_err(storage_error)?
            .map(|r| decode_json(&r.map_err(storage_error)?, "project plugin grant"))
            .collect()
    }
    fn authorize_plugin(
        &self,
        input: &InspectInput,
        execute: bool,
    ) -> Result<Authorization, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let ctx = &self.connection.1;
        sqlite_access::require(&tx, ctx, &input.target.project_id, ProjectAction::Read)?;
        if execute {
            for action in [ProjectAction::Edit, ProjectAction::RunJobs] {
                sqlite_access::require(&tx, ctx, &input.target.project_id, action)?;
            }
        }
        let grant = read(&tx, &input.target)?
            .filter(|g| g.enabled && g.operations.contains(&input.operation_name))
            .ok_or_else(access::denied)?;
        let principal_revision = if ctx.caller().is_owner() {
            None
        } else {
            Some(
                tx.query_row(
                    "SELECT CAST(revision AS TEXT) FROM access_principals WHERE principal_id=?1",
                    [ctx.caller().principal_id()],
                    |r| r.get(0),
                )
                .map_err(storage_error)?,
            )
        };
        Ok(Authorization {
            grant,
            principal_revision,
        })
    }
}
