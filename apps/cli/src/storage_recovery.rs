//! Local owner recovery that remains available when managed SQLite cannot open.
use clap::Subcommand;
use opencut_automation::{CallSource, Caller, ExecutionContext, Response};
use opencut_contracts::EditorError;
use opencut_editor_application::{access::denied, budget};
use opencut_native_services::budget::NativeBudget;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub enum Command {
    /// Inspect or reconfigure the native admission ledger without SQLite.
    Recovery {
        #[command(subcommand)]
        command: RecoveryCommand,
    },
}

#[derive(Subcommand)]
pub enum RecoveryCommand {
    /// Read the owner budget policy and reservations without opening SQLite.
    Inspect,
    /// Restore an unreachable policy with expected revision and idempotency key.
    Configure {
        #[arg(long)]
        input: PathBuf,
    },
    /// Release the preallocated repair file after all normal hosts stop.
    Enter {
        #[arg(long)]
        input: PathBuf,
    },
    /// Cancel one existing job after fencing normal hosts and live attempt writers.
    Cancel { job_id: String },
    /// Release eligible upload staging while the bounded repair gate is active.
    Release { upload_id: String },
    /// Reallocate and sync the repair file before resuming normal writes.
    Exit {
        #[arg(long)]
        input: PathBuf,
    },
}

pub fn execute(database: &Path, credential: Option<&Path>, command: &Command) -> Response {
    let context = ExecutionContext::local(CallSource::Cli);
    let result = (|| -> Result<Value, EditorError> {
        // This is local OS-owner authority only. A principal credential must be
        // checked against SQLite, which may be unavailable in recovery mode.
        if credential.is_some() {
            return Err(denied());
        }
        if database == Path::new(":memory:") {
            return Err(EditorError::new(
                "CAPABILITY_UNAVAILABLE",
                "Storage recovery requires a file-backed workspace",
            ));
        }
        let native = NativeBudget(database);
        match command {
            Command::Recovery {
                command: RecoveryCommand::Inspect,
            } => Ok(json!(budget::inspect(&Caller::LocalOwner, &native, false)?)),
            Command::Recovery {
                command: RecoveryCommand::Configure { input },
            } => {
                let workspace = database
                    .parent()
                    .filter(|path| !path.as_os_str().is_empty())
                    .unwrap_or_else(|| Path::new("."));
                std::fs::create_dir_all(workspace).map_err(|_| {
                    EditorError::new("STORAGE_ERROR", "Could not create recovery workspace")
                })?;
                let input: budget::Configure = serde_json::from_value(super::read_json(input)?)
                    .map_err(|_| {
                        EditorError::new("INVALID_ARGUMENT", "Invalid storage policy request")
                    })?;
                Ok(json!(budget::configure(&context, &native, input)?))
            }
            Command::Recovery {
                command: RecoveryCommand::Enter { input },
            }
            | Command::Recovery {
                command: RecoveryCommand::Exit { input },
            } => {
                let action = if matches!(
                    command,
                    Command::Recovery {
                        command: RecoveryCommand::Enter { .. }
                    }
                ) {
                    budget::RepairAction::Enter
                } else {
                    budget::RepairAction::Exit
                };
                let input: budget::RepairRequest = serde_json::from_value(super::read_json(input)?)
                    .map_err(|_| {
                        EditorError::new("INVALID_ARGUMENT", "Invalid repair transition request")
                    })?;
                Ok(json!(budget::repair_transition(
                    &context, &native, action, input
                )?))
            }
            Command::Recovery {
                command: RecoveryCommand::Cancel { job_id },
            } => {
                let mut app =
                    opencut_sqlite_store::EditorApplication::open_managed_repair(database)?;
                app.set_execution_context(context.clone());
                Ok(json!(app.repair_cancel_job(job_id)?))
            }
            Command::Recovery {
                command: RecoveryCommand::Release { upload_id },
            } => {
                let mut app =
                    opencut_sqlite_store::EditorApplication::open_managed_repair(database)?;
                app.set_execution_context(context.clone());
                let uploads_root = app
                    .asset_store_root()
                    .parent()
                    .ok_or_else(|| EditorError::new("STORAGE_ERROR", "Invalid asset root"))?
                    .join("uploads");
                Ok(json!(opencut_native_services::uploads::release(
                    &uploads_root,
                    &mut app,
                    opencut_editor_application::uploads::UploadInput {
                        upload_id: upload_id.clone(),
                    },
                )?))
            }
        }
    })();
    Response::from_result_with_id(context.request_id().to_owned(), result)
}
