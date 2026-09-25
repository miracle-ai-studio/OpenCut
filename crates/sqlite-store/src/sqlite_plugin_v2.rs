//! Plugin V2 project grants; never read caller-supplied rights from request data.
use super::*;
use opencut_editor_application::plugin_v2::{
    self, Authorization, Grant, GrantTarget, PresetGrantStore, PresetInput, SetGrant,
};

pub(super) const MIGRATION: &str = "
CREATE TABLE project_preset_grants(
 project_id TEXT NOT NULL REFERENCES projects(project_id),
 plugin_id TEXT NOT NULL, version TEXT NOT NULL,
 revision INTEGER NOT NULL CHECK(revision>0), grant_json TEXT NOT NULL,
 PRIMARY KEY(project_id,plugin_id,version));
UPDATE metadata SET value='22' WHERE key='schema_version' AND value='21';
";

fn read(c: &Connection, target: &GrantTarget) -> Result<Option<Grant>, EditorError> {
    let value: Option<String> = c.query_row(
        "SELECT grant_json FROM project_preset_grants WHERE project_id=?1 AND plugin_id=?2 AND version=?3",
        params![target.project_id,target.id,target.version], |r| r.get(0),
    ).optional().map_err(storage_error)?;
    value.map(|v| decode_json(&v, "preset grant")).transpose()
}

// Grants belong to the project identity, independent of its document version.
fn project_revision(c: &Connection, project: &str) -> Result<String, EditorError> {
    let (raw, revision): (String, String) = c.query_row(
        "SELECT r.document_json,p.head_revision FROM projects p JOIN revisions r ON r.revision=p.head_revision WHERE p.project_id=?1",
        [project], |row| Ok((row.get(0)?,row.get(1)?)),
    ).optional().map_err(storage_error)?.ok_or_else(||EditorError::new("NOT_FOUND","Project does not exist"))?;
    let id = match sqlite_project_v3::decode_versioned_project(&raw)? {
        sqlite_project_v3::VersionedProjectDocument::Legacy(doc) => doc.id,
        sqlite_project_v3::VersionedProjectDocument::V3(doc) => doc.base.id,
    };
    if id != project {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Project identity differs from grant scope",
        ));
    }
    Ok(revision)
}

impl PresetGrantStore for EditorApplication {
    fn set_preset_grant(&mut self, input: &SetGrant) -> Result<Grant, EditorError> {
        sqlite_access::owner(&self.connection.1)?;
        plugin_v2::validate_grant(input)?;
        let context = self.connection.1.clone();
        let scope = encode_json(&input.target, "preset grant target")?;
        let identity = request_hash("project_preset_grant", &scope, input)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        sqlite_access::owner(&context)?;
        if let Some(result) = load_idempotent(
            &tx,
            "project_preset_grant",
            &scope,
            &input.idempotency_key,
            &identity,
            "preset_grant",
        )? {
            return decode_json(&result, "preset grant receipt");
        }
        let project_revision = project_revision(&tx, &input.target.project_id)?;
        let previous = read(&tx, &input.target)?;
        let revision = previous.as_ref().map_or("0", |g| g.revision.as_str());
        if revision != input.expected_revision {
            return Err(EditorError::new(
                "REVISION_CONFLICT",
                "Preset grant revision changed",
            ));
        }
        if previous.is_none() {
            let count: usize = tx
                .query_row(
                    "SELECT COUNT(*) FROM project_preset_grants WHERE project_id=?1",
                    [&input.target.project_id],
                    |r| r.get(0),
                )
                .map_err(storage_error)?;
            if count >= 64 {
                return Err(EditorError::new(
                    "CAPABILITY_UNAVAILABLE",
                    "Preset grant capacity reached",
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
            preset_names: input.preset_names.clone(),
            enabled: input.enabled,
            revision: next.to_string(),
        };
        let result = encode_json(&grant, "preset grant")?;
        tx.execute("INSERT INTO project_preset_grants(project_id,plugin_id,version,revision,grant_json) VALUES (?1,?2,?3,?4,?5) ON CONFLICT(project_id,plugin_id,version) DO UPDATE SET revision=excluded.revision,grant_json=excluded.grant_json",
            params![input.target.project_id,input.target.id,input.target.version,next,result]).map_err(storage_error)?;
        tx.execute("INSERT INTO idempotency(operation_family,scope,idempotency_key,request_hash,result_kind,result_json) VALUES ('project_preset_grant',?1,?2,?3,'preset_grant',?4)",
            params![scope,input.idempotency_key,identity.current_digest(),result]).map_err(idempotency_insert_error)?;
        tx.execute("INSERT INTO outbox(event_id,project_id,revision,event_type,payload_json) VALUES (?1,?2,?3,'project.preset_grant.updated',?4)",
            params![format!("evt_{}",Uuid::new_v4().simple()),input.target.project_id,project_revision,result]).map_err(storage_error)?;
        sqlite_audit::insert(
            &tx,
            &context,
            "project.preset_grant.updated",
            sqlite_audit::Links {
                project_id: Some(&input.target.project_id),
                revision: Some(&project_revision),
                ..Default::default()
            },
        )?;
        tx.commit().map_err(storage_error)?;
        Ok(grant)
    }

    fn preset_grants(&self, project_id: &str) -> Result<Vec<Grant>, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        sqlite_access::require(&tx, &self.connection.1, project_id, ProjectAction::Read)?;
        project_revision(&tx, project_id)?;
        let mut statement = tx.prepare("SELECT grant_json FROM project_preset_grants WHERE project_id=?1 ORDER BY plugin_id,version LIMIT 64").map_err(storage_error)?;
        statement
            .query_map([project_id], |r| r.get::<_, String>(0))
            .map_err(storage_error)?
            .map(|r| decode_json(&r.map_err(storage_error)?, "preset grant"))
            .collect()
    }

    fn authorize_preset(
        &self,
        input: &PresetInput,
        execute: bool,
    ) -> Result<Authorization, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let ctx = &self.connection.1;
        sqlite_access::require(&tx, ctx, &input.target.project_id, ProjectAction::Read)?;
        if execute {
            sqlite_access::require(&tx, ctx, &input.target.project_id, ProjectAction::Edit)?;
            sqlite_access::require(&tx, ctx, &input.target.project_id, ProjectAction::RunJobs)?;
        }
        let grant = read(&tx, &input.target)?
            .filter(|g| g.enabled && g.preset_names.contains(&input.preset_name))
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
