mod access;
mod blocking_owned;
mod denials;
mod event_streams;
mod http;
mod http_identity;
mod job_wait;
mod mcp;
mod mcp_context;
mod mcp_http;
mod mcp_replay;
mod mcp_sessions;
mod media_http;
mod preview_http;
mod storage_recovery;
mod upload_cli;
mod upload_http;

use clap::{Parser, Subcommand};
use opencut_automation::{Automation, Response, capabilities, exit_code, tool_specs};
use opencut_contracts::EditorError;
use serde_json::{Value, json};
use std::{
    fs::File,
    io::{self, Read},
    path::PathBuf,
};

#[derive(Parser)]
#[command(
    name = "opencut",
    version,
    about = "Transactional video editing for people, scripts and agents"
)]
struct Cli {
    #[arg(long, global = true, default_value = ".opencut/workspace.sqlite")]
    database: PathBuf,
    /// Use a restricted principal credential; default local invocation is OS-owner authority.
    #[arg(long, global = true)]
    credential_file: Option<PathBuf>,
    /// Send call/domain requests to a long-lived HTTP host; --credential-file supplies its bearer token.
    #[arg(long, global = true)]
    endpoint: Option<String>,
    /// Emit a single JSON envelope. Progress and diagnostics never use stdout.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Local owner recovery of durable storage policy without opening SQLite.
    Storage {
        #[command(subcommand)]
        command: storage_recovery::Command,
    },
    Upload {
        #[command(subcommand)]
        command: upload_cli::Command,
    },
    Access {
        #[command(subcommand)]
        command: access::Command,
    },
    Plugin {
        #[command(subcommand)]
        command: PluginCommand,
    },
    Worker {
        #[command(subcommand)]
        command: WorkerCommand,
    },
    Job {
        #[command(subcommand)]
        command: JobCommand,
    },
    Render {
        #[command(subcommand)]
        command: RenderCommand,
    },
    /// Start an authenticated local HTTP host for the shared editor API.
    Serve {
        #[arg(long, default_value = "127.0.0.1:8788")]
        listen: std::net::SocketAddr,
        #[arg(long, default_value = ".opencut/api-token")]
        token_file: PathBuf,
        #[arg(long)]
        allow_origin: Vec<String>,
        #[arg(long, default_value_t=300, value_parser=clap::value_parser!(u64).range(1..=3600))]
        mcp_session_timeout_seconds: u64,
        #[arg(long, value_enum, default_value_t = mcp::ToolSurface::Domains)]
        tool_surface: mcp::ToolSurface,
    },
    System {
        #[command(subcommand)]
        command: SystemCommand,
    },
    Project {
        #[command(subcommand)]
        command: ProjectCommand,
    },
    Edit {
        #[command(subcommand)]
        command: EditCommand,
    },
    Events {
        #[arg(long, default_value_t = 0)]
        after: i64,
        #[arg(long, default_value_t = 100)]
        limit: usize,
        #[arg(long)]
        project_id: Option<String>,
        #[command(flatten)]
        stream: event_streams::Options,
    },
    /// Invoke a registered operation using the same JSON arguments as MCP.
    Call {
        name: String,
        #[arg(long)]
        input: PathBuf,
    },
    /// Invoke an action by domain; use `system domains` to discover actions.
    Domain {
        domain: String,
        action: String,
        #[arg(long)]
        input: PathBuf,
    },
    Mcp {
        #[command(subcommand)]
        command: McpCommand,
    },
}

#[derive(Subcommand)]
enum SystemCommand {
    Capabilities,
    Schema,
    /// Compact action catalog; add --domain for one domain and --schemas for exact contracts.
    Domains {
        #[arg(long)]
        domain: Option<String>,
        #[arg(long)]
        schemas: bool,
    },
    /// JSON Schema for versioned CLI NDJSON records.
    EventsSchema,
}

#[derive(Subcommand)]
enum WorkerCommand {
    Run {
        #[arg(long)]
        once: bool,
    },
}

#[derive(Subcommand)]
enum JobCommand {
    Start {
        #[arg(long)]
        input: PathBuf,
    },
    Get {
        job_id: String,
    },
    /// Wait for a durable job; failed/cancelled jobs and timeout exit nonzero.
    Wait {
        job_id: String,
        #[arg(long, default_value_t = 300)]
        timeout_seconds: u64,
        /// Request durable cancellation only if this wait is interrupted.
        #[arg(long)]
        cancel_on_interrupt: bool,
    },
    List,
    Cancel {
        job_id: String,
    },
    Events {
        #[arg(long, default_value_t = 0)]
        after_sequence: i64,
        #[arg(long, default_value_t = 100)]
        limit: usize,
        #[arg(long)]
        job_id: Option<String>,
        #[command(flatten)]
        stream: event_streams::Options,
    },
}

#[derive(Subcommand)]
enum RenderCommand {
    Preflight {
        #[arg(long)]
        input: PathBuf,
    },
}

#[derive(Subcommand)]
enum PluginCommand {
    List,
    Inspect {
        id: String,
        version: String,
    },
    Install {
        #[arg(long)]
        input: PathBuf,
    },
    Plan {
        #[arg(long)]
        input: PathBuf,
    },
}

#[derive(Subcommand)]
enum ProjectCommand {
    History {
        project_id: String,
        #[arg(long)]
        from_revision: Option<String>,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    Revision {
        project_id: String,
        revision: String,
    },
    Restore {
        project_id: String,
        #[arg(long)]
        input: PathBuf,
    },
    Create {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        idempotency_key: String,
    },
    List,
    Inspect {
        project_id: String,
    },
}

#[derive(Subcommand)]
enum EditCommand {
    Plan {
        #[arg(long)]
        project: String,
        #[arg(long)]
        input: PathBuf,
    },
    Apply {
        #[arg(long)]
        project: String,
        #[arg(long)]
        input: PathBuf,
    },
}

#[derive(Subcommand)]
enum McpCommand {
    Serve {
        #[arg(long, default_value = "stdio", value_parser = ["stdio","streamable-http"])]
        transport: String,
        #[arg(long)]
        listen: Option<std::net::SocketAddr>,
        #[arg(long)]
        token_file: Option<PathBuf>,
        #[arg(long)]
        allow_origin: Vec<String>,
        #[arg(long,value_parser=clap::value_parser!(u64).range(1..=3600))]
        session_timeout_seconds: Option<u64>,
        #[arg(long, value_enum, default_value_t = mcp::ToolSurface::Domains)]
        tool_surface: mcp::ToolSurface,
    },
}

#[tokio::main]
async fn main() {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            let args: Vec<_> = std::env::args_os().collect();
            let ndjson = args.iter().any(|arg| arg == "--format=ndjson")
                || args
                    .windows(2)
                    .any(|pair| pair[0] == "--format" && pair[1] == "ndjson");
            if error.use_stderr() && ndjson {
                use opencut_automation::event_stream::{StreamRecord, StreamRecordData};
                let record = StreamRecord::new(StreamRecordData::Error {
                    cursor: None,
                    error: EditorError::new("INVALID_ARGUMENT", error.to_string()),
                });
                use std::io::Write;
                let _ = writeln!(
                    std::io::stdout().lock(),
                    "{}",
                    serde_json::to_string(&record).expect("serializable error")
                );
                std::process::exit(2);
            }
            if error.use_stderr() && args.iter().any(|arg| arg == "--json") {
                let response = opencut_automation::response(Err(EditorError::new(
                    "INVALID_ARGUMENT",
                    error.to_string(),
                )));
                println!(
                    "{}",
                    serde_json::to_string(&response).expect("serializable error")
                );
                std::process::exit(2);
            }
            error.exit();
        }
    };
    if let Some(endpoint) = &cli.endpoint {
        emit_response(remote_call(&cli, endpoint), cli.json);
        return;
    }
    if let Command::Storage { command } = &cli.command {
        // Ordinary authentication and denial journaling both open SQLite. A
        // recovery entry must remain reachable when SQLite cannot allocate.
        emit_response(
            storage_recovery::execute(&cli.database, cli.credential_file.as_deref(), command),
            cli.json,
        );
        return;
    }
    let caller = match access::caller(&cli.database, cli.credential_file.as_deref()) {
        Ok(caller) => caller,
        Err(error) => {
            let code = exit_code(&error);
            println!(
                "{}",
                serde_json::to_string(&denials::response(
                    &cli.database,
                    None,
                    opencut_automation::CallSource::Cli,
                    opencut_automation::denials::DeniedOperation::Authentication,
                    opencut_automation::denials::DenialReason::AuthenticationRejected,
                    opencut_automation::response(Err(error))
                ))
                .expect("serializable error")
            );
            std::process::exit(code);
        }
    };
    if !caller.is_owner()
        && (matches!(&cli.command, Command::Worker { .. } | Command::Serve { .. })
            || matches!(&cli.command, Command::Mcp {command:McpCommand::Serve {transport,..}} if transport == "streamable-http"))
    {
        // The network host still uses the separately configured owner bearer.
        println!(
            "{}",
            serde_json::to_string(&denials::response(
                &cli.database,
                Some(&caller),
                opencut_automation::CallSource::Cli,
                opencut_automation::denials::DeniedOperation::HostStart,
                opencut_automation::denials::DenialReason::AuthorizationDenied,
                opencut_automation::response(Err(opencut_editor_application::access::denied()))
            ))
            .expect("serializable error")
        );
        std::process::exit(3);
    }
    let stream = match &cli.command {
        Command::Events {
            after,
            limit,
            project_id,
            stream,
        } if stream.format.is_some() => Some((
            opencut_automation::event_stream::EventScope::Projects {
                project_id: project_id.clone(),
            },
            *after,
            *limit,
            stream.clone(),
        )),
        Command::Job {
            command:
                JobCommand::Events {
                    after_sequence,
                    limit,
                    job_id,
                    stream,
                },
        } if stream.format.is_some() => Some((
            opencut_automation::event_stream::EventScope::Jobs {
                job_id: job_id.clone(),
            },
            *after_sequence,
            *limit,
            stream.clone(),
        )),
        _ => None,
    };
    if let Some((scope, after, limit, options)) = stream {
        let timeout = options.timeout_seconds;
        let deadline = async move {
            if let Some(seconds) = timeout {
                tokio::time::sleep(std::time::Duration::from_secs(seconds)).await;
            } else {
                std::future::pending::<()>().await;
            }
        };
        let code = tokio::select! {
            result=event_streams::run(cli.database.clone(),caller.clone(),scope,after,limit,options)=>result,
            _=tokio::signal::ctrl_c()=>{eprintln!("Event observation interrupted; durable jobs were not cancelled");130},
            _=deadline=>{eprintln!("Event observation timed out; resume from the last fully processed cursor");124},
        };
        // Exit also stops an output write blocked by an unread pipe. Do not wait
        // for Tokio's stdout blocking task or synthesize a line into that pipe.
        std::process::exit(code);
    }
    if let Command::Worker {
        command: WorkerCommand::Run { once },
    } = &cli.command
    {
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_stop = stop.clone();
        let database = cli.database.clone();
        let once = *once;
        let mut worker = tokio::task::spawn_blocking(move || {
            opencut_job_runtime::run(&database, once, worker_stop)
        });
        let outcome = tokio::select! {
            result=&mut worker=>result,
            _=tokio::signal::ctrl_c()=>{
                stop.store(true,std::sync::atomic::Ordering::SeqCst);
                worker.await
            }
        };
        match outcome.unwrap_or_else(|e| Err(EditorError::new("INTERNAL_ERROR", e.to_string()))) {
            Ok(()) => return,
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(8);
            }
        }
    }
    if let Command::Serve {
        listen,
        token_file,
        allow_origin,
        mcp_session_timeout_seconds,
        tool_surface,
    } = &cli.command
    {
        if let Err(error) = http::serve(
            &cli.database,
            *listen,
            token_file,
            allow_origin.clone(),
            *mcp_session_timeout_seconds,
            *tool_surface,
        )
        .await
        {
            eprintln!("{error}");
            std::process::exit(8);
        }
        return;
    }
    if let Command::Mcp {
        command:
            McpCommand::Serve {
                transport,
                listen,
                token_file,
                allow_origin,
                session_timeout_seconds,
                tool_surface,
            },
    } = &cli.command
    {
        if transport == "stdio"
            && (listen.is_some()
                || token_file.is_some()
                || !allow_origin.is_empty()
                || session_timeout_seconds.is_some())
        {
            let response = opencut_automation::response(Err(EditorError::new(
                "INVALID_ARGUMENT",
                "HTTP options require --transport streamable-http",
            )));
            if cli.json {
                println!(
                    "{}",
                    serde_json::to_string(&response).expect("serializable error")
                );
            } else {
                eprintln!("HTTP options require --transport streamable-http");
            }
            std::process::exit(2);
        }
        let result = if transport == "streamable-http" {
            http::serve(
                &cli.database,
                listen.unwrap_or_else(|| "127.0.0.1:8788".parse().expect("valid address")),
                token_file
                    .as_deref()
                    .unwrap_or(std::path::Path::new(".opencut/api-token")),
                allow_origin.clone(),
                session_timeout_seconds.unwrap_or(300),
                *tool_surface,
            )
            .await
        } else {
            mcp::serve(&cli.database, caller.clone(), *tool_surface).await
        };
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(8);
        }
        // Stdio peer closure must not wait on an unread Tokio stdout helper.
        std::process::exit(0);
    }
    let result = if let Command::Job {
        command:
            JobCommand::Wait {
                job_id,
                timeout_seconds,
                cancel_on_interrupt,
            },
    } = &cli.command
    {
        job_wait::run(
            &cli.database,
            job_id,
            *timeout_seconds,
            *cancel_on_interrupt,
            caller.clone(),
        )
        .await
    } else {
        execute(&cli, caller.clone())
    };
    let response = match result {
        Ok(response) => response,
        Err(error) => opencut_automation::response(Err(error)),
    };
    let response = denials::response(
        &cli.database,
        Some(&caller),
        opencut_automation::CallSource::Cli,
        opencut_automation::denials::DeniedOperation::CliRequest,
        opencut_automation::denials::DenialReason::AuthorizationDenied,
        response,
    );
    emit_response(response, cli.json);
}

fn emit_response(response: Response, json: bool) {
    let code = response
        .error
        .as_ref()
        .map(|error| {
            if error.code == "USER_INTERRUPTED" {
                130
            } else {
                exit_code(error)
            }
        })
        .unwrap_or(0);
    let output = if json {
        serde_json::to_string(&response)
    } else {
        serde_json::to_string_pretty(&response)
    };
    match output {
        Ok(output) => println!("{output}"),
        Err(error) => {
            eprintln!("Unable to serialize response: {error}");
            std::process::exit(8);
        }
    }
    if code != 0 {
        std::process::exit(code);
    }
}

fn execute(cli: &Cli, caller: opencut_automation::Caller) -> Result<Response, EditorError> {
    if let Command::Upload { command } = &cli.command {
        return upload_cli::execute(&cli.database, caller, command);
    }
    if let Command::Access { command } = &cli.command {
        return access::execute(&cli.database, caller, command);
    }
    match &cli.command {
        Command::System {
            command: SystemCommand::Capabilities,
        } => return Ok(opencut_automation::response(Ok(capabilities()))),
        Command::System {
            command: SystemCommand::EventsSchema,
        } => {
            return Ok(opencut_automation::response(Ok(
                opencut_automation::event_stream::schema(),
            )));
        }
        Command::System {
            command: SystemCommand::Schema,
        } => return Ok(opencut_automation::response(Ok(json!(tool_specs())))),
        Command::System {
            command: SystemCommand::Domains { domain, schemas },
        } => {
            return Ok(opencut_automation::response(Ok(json!(
                opencut_automation::domains::catalog(domain.as_deref(), *schemas)?
            ))));
        }
        _ => {}
    }
    let (name, arguments) = match &cli.command {
        Command::Render { command } => match command {
            RenderCommand::Preflight { input } => ("opencut_render_preflight", read_json(input)?),
        },
        Command::Plugin { command } => match command {
            PluginCommand::List => ("opencut_plugin_list", json!({})),
            PluginCommand::Inspect { id, version } => {
                ("opencut_plugin_inspect", json!({"id":id,"version":version}))
            }
            PluginCommand::Install { input } => ("opencut_plugin_install", read_json(input)?),
            PluginCommand::Plan { input } => ("opencut_plugin_plan", read_json(input)?),
        },
        Command::Job { command } => match command {
            JobCommand::Start { input } => (
                if caller.is_owner() {
                    "opencut_job_start"
                } else {
                    "opencut_job_submit"
                },
                read_json(input)?,
            ),
            JobCommand::Get { job_id } => (
                if caller.is_owner() {
                    "opencut_job_get"
                } else {
                    "opencut_job_view"
                },
                json!({"job_id":job_id}),
            ),
            JobCommand::Wait { .. } => unreachable!("wait handled before ordinary dispatch"),
            JobCommand::List => (
                if caller.is_owner() {
                    "opencut_job_list"
                } else {
                    "opencut_job_query"
                },
                json!({}),
            ),
            JobCommand::Cancel { job_id } => (
                if caller.is_owner() {
                    "opencut_job_cancel"
                } else {
                    "opencut_job_request_cancel"
                },
                json!({"job_id":job_id}),
            ),
            JobCommand::Events {
                after_sequence,
                limit,
                job_id,
                ..
            } => {
                if caller.is_owner() {
                    (
                        "opencut_job_events",
                        json!({"after_sequence":after_sequence,"limit":limit,"job_id":job_id}),
                    )
                } else {
                    if *after_sequence != 0 {
                        return Err(EditorError::new(
                            "INVALID_ARGUMENT",
                            "Restricted task observation uses opencut_job_changes cursors or --format ndjson",
                        ));
                    }
                    (
                        "opencut_job_changes",
                        json!({"limit":limit,"job_id":job_id}),
                    )
                }
            }
        },
        Command::Project { command } => match command {
            ProjectCommand::History {
                project_id,
                from_revision,
                limit,
            } => (
                "opencut_project_history",
                json!({"project_id":project_id,"from_revision":from_revision,"limit":limit}),
            ),
            ProjectCommand::Revision {
                project_id,
                revision,
            } => (
                "opencut_revision_inspect",
                json!({"project_id":project_id,"revision":revision}),
            ),
            ProjectCommand::Restore { project_id, input } => (
                "opencut_revision_restore",
                json!({"project_id":project_id,"request":read_json(input)?}),
            ),
            ProjectCommand::Create {
                input,
                idempotency_key,
            } => (
                "opencut_project_create",
                json!({"project":read_json(input)?, "idempotency_key":idempotency_key}),
            ),
            ProjectCommand::List => ("opencut_project_list", json!({})),
            ProjectCommand::Inspect { project_id } => {
                ("opencut_project_inspect", json!({"project_id":project_id}))
            }
        },
        Command::Edit { command } => match command {
            EditCommand::Plan { project, input } => (
                "opencut_edit_plan",
                json!({"project_id":project,"request":read_json(input)?}),
            ),
            EditCommand::Apply { project, input } => (
                "opencut_edit_apply",
                json!({"project_id":project,"request":read_json(input)?}),
            ),
        },
        Command::Events {
            after,
            limit,
            project_id,
            ..
        } => {
            if project_id.is_some() {
                return Err(EditorError::new(
                    "INVALID_ARGUMENT",
                    "project-id filtering requires --format ndjson; use opencut_event_changes for paged JSON",
                ));
            }
            (
                "opencut_events_read",
                json!({"after_sequence":after,"limit":limit}),
            )
        }
        Command::Call { name, input } => (name.as_str(), read_json(input)?),
        Command::Domain {
            domain,
            action,
            input,
        } => {
            let name = format!("opencut_{domain}");
            if !opencut_automation::domains::is_domain_tool(&name) {
                return Err(EditorError::new(
                    "INVALID_ARGUMENT",
                    "Unknown operation domain",
                ));
            }
            let arguments = json!({"action":action,"input":read_json(input)?});
            let mut automation = Automation::open(&cli.database)?
                .with_source(opencut_automation::CallSource::Cli)
                .with_caller(caller);
            return Ok(automation.call(&name, arguments));
        }
        _ => unreachable!("non-editing entry points handled before database access"),
    };
    let mut automation = Automation::open(&cli.database)?
        .with_source(opencut_automation::CallSource::Cli)
        .with_caller(caller);
    Ok(automation.call(name, arguments))
}

fn remote_call(cli: &Cli, endpoint: &str) -> Response {
    let prepared = (|| {
        let (name, input) = match &cli.command {
            Command::Call { name, input } => (name.clone(), read_json(input)?),
            Command::Domain {
                domain,
                action,
                input,
            } => (
                format!("opencut_{domain}"),
                json!({"action":action,"input":read_json(input)?}),
            ),
            _ => {
                return Err(EditorError::new(
                    "INVALID_ARGUMENT",
                    "--endpoint supports call and domain commands",
                ));
            }
        };
        let path = cli.credential_file.as_ref().ok_or_else(|| {
            EditorError::new(
                "INVALID_ARGUMENT",
                "--endpoint requires --credential-file for the host bearer token",
            )
        })?;
        if std::fs::metadata(path)
            .map_err(|_| EditorError::new("INVALID_ARGUMENT", "Could not read credential file"))?
            .len()
            > 16_384
        {
            return Err(EditorError::new(
                "INVALID_ARGUMENT",
                "Credential file is too large",
            ));
        }
        let token = std::fs::read_to_string(path)
            .map_err(|_| EditorError::new("INVALID_ARGUMENT", "Could not read credential file"))?;
        if token.trim().is_empty() {
            return Err(EditorError::new(
                "INVALID_ARGUMENT",
                "Credential file is empty",
            ));
        }
        let client = opencut_sdk::Client::new(endpoint, token.trim())
            .map_err(|e| EditorError::new("INVALID_ARGUMENT", e.to_string()))?;
        Ok((client, name, input))
    })();
    let (client, name, input) = match prepared {
        Ok(value) => value,
        Err(error) => return opencut_automation::response(Err(error)),
    };
    match client.call_registered(&name, &input) {
        Ok(result) => {
            let mut response = Response::from_result_with_id(result.request_id, Ok(result.data));
            response.warnings = result.warnings;
            response
        }
        Err(opencut_sdk::Error::Api(failure)) => {
            let mut response =
                Response::from_result_with_id(failure.request_id, Err(failure.error));
            response.warnings = failure.warnings;
            response
        }
        Err(error) => opencut_automation::response(Err(EditorError::new(
            "TRANSPORT_ERROR",
            error.to_string(),
        ))),
    }
}

fn read_json(path: &PathBuf) -> Result<Value, EditorError> {
    let reader: Box<dyn Read> = if path.as_os_str() == "-" {
        Box::new(io::stdin())
    } else {
        Box::new(File::open(path).map_err(|e| {
            EditorError::new("INVALID_ARGUMENT", format!("{}: {e}", path.display()))
        })?)
    };
    let mut bytes = Vec::new();
    reader
        .take(2_097_153)
        .read_to_end(&mut bytes)
        .map_err(|e| EditorError::new("INVALID_ARGUMENT", e.to_string()))?;
    if bytes.len() > 2_097_152 {
        return Err(EditorError::new("INVALID_ARGUMENT", "Input exceeds 2 MiB"));
    }
    serde_json::from_slice(&bytes).map_err(|e| EditorError::new("INVALID_ARGUMENT", e.to_string()))
}
