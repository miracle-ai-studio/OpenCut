//! Local authority administration. Issued secrets go only to a new private file.
use clap::Subcommand;
use opencut_automation::{CallSource, Caller, ExecutionContext, Response};
use opencut_contracts::EditorError;
use opencut_editor_application::access::{PrincipalPolicy, denied};
use opencut_sqlite_store::EditorApplication;
use serde_json::json;
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Subcommand)]
pub enum Command {
    /// Inspect this credential's current permissions.
    Inspect,
    /// Query committed policy and credential history as owner; never returns secrets.
    Events {
        #[arg(long)]
        input: PathBuf,
    },
    /// Atomically replace a principal policy using its expected revision.
    SetPolicy {
        #[arg(long)]
        input: PathBuf,
    },
    /// Inspect a policy as the local owner.
    GetPolicy { principal_id: String },
    /// Issue a credential into a new file, never stdout. Parent directory must exist.
    Issue {
        principal_id: String,
        #[arg(long)]
        output: PathBuf,
    },
    /// Revoke one credential; repeat revocation is safe.
    Revoke { credential_id: String },
}
pub fn caller(database: &Path, file: Option<&Path>) -> Result<Caller, EditorError> {
    let Some(file) = file else {
        return Ok(Caller::LocalOwner);
    };
    let mut secret = String::new();
    std::fs::File::open(file)
        .and_then(|file| file.take(66).read_to_string(&mut secret))
        .map_err(|_| denied())?;
    // Credential files are one exact secret plus an optional Unix newline.
    EditorApplication::open(database)?.authenticate(secret.strip_suffix('\n').unwrap_or(&secret))
}
pub fn execute(
    database: &Path,
    caller: Caller,
    command: &Command,
) -> Result<Response, EditorError> {
    let mut app = EditorApplication::open(database)?;
    let context = ExecutionContext::for_caller(CallSource::Cli, caller);
    let request_id = context.request_id().to_owned();
    app.set_execution_context(context.clone());
    let result = (|| {
        Ok(match command {
            Command::Inspect => json!(app.access_view()?),
            Command::Events { input } => {
                let query = serde_json::from_value(super::read_json(input)?).map_err(|_| {
                    EditorError::new("INVALID_ARGUMENT", "Invalid access audit query")
                })?;
                json!(app.access_audit_query(query)?)
            }
            Command::GetPolicy { principal_id } => json!(app.principal_policy(principal_id)?),
            Command::SetPolicy { input } => {
                let policy: PrincipalPolicy = serde_json::from_value(super::read_json(input)?)
                    .map_err(|_| {
                        EditorError::new("INVALID_ARGUMENT", "Invalid principal policy")
                    })?;
                json!(app.set_principal_policy(policy)?)
            }
            Command::Revoke { credential_id } => {
                app.revoke_credential(credential_id)?;
                json!({"credential_id":credential_id,"revoked":true})
            }
            Command::Issue {
                principal_id,
                output,
            } => {
                // Fail before minting if a file exists or cannot be created. Never overwrite.
                app.principal_policy(principal_id)?;
                let mut options = std::fs::OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                let mut file = options.open(output).map_err(|_| {
                    EditorError::new("STORAGE_ERROR", "Could not create a new credential file")
                })?;
                let issued = match app.issue_credential(principal_id) {
                    Ok(issued) => issued,
                    Err(error) => {
                        drop(file);
                        let _ = std::fs::remove_file(output);
                        return Err(error);
                    }
                };
                if writeln!(file, "{}", issued.secret)
                    .and_then(|_| file.sync_all())
                    .is_err()
                {
                    // Do not leave a partially written usable credential behind.
                    app.revoke_credential(&issued.credential_id)?;
                    drop(file);
                    let _ = std::fs::remove_file(output);
                    return Err(EditorError::new(
                        "STORAGE_ERROR",
                        "Credential file write failed; credential revoked",
                    ));
                }
                json!({"principal_id":issued.principal_id,"credential_id":issued.credential_id,"credential_file":output})
            }
        })
    })();
    Ok(opencut_automation::denials::observe_response(
        &app,
        Some(context.caller()),
        CallSource::Cli,
        opencut_automation::denials::DeniedOperation::Administration,
        opencut_automation::denials::DenialReason::AuthorizationDenied,
        Response::from_result_with_id(request_id, result),
    ))
}
