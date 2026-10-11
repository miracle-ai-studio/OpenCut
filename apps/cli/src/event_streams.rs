//! Bounded NDJSON observation. Only explicit job commands can cancel durable work.
use clap::{Args, ValueEnum};
use opencut_automation::event_stream::*;
use opencut_contracts::EditorError;
use std::path::PathBuf;
use tokio::io::{AsyncWriteExt, Stdout};

#[derive(Clone, Debug, ValueEnum)]
pub enum OutputFormat {
    Ndjson,
}
#[derive(Clone, Debug, Args)]
pub struct Options {
    /// Follow committed events until interrupted. Requires --format ndjson.
    #[arg(long, requires = "format")]
    pub watch: bool,
    #[arg(long, value_enum, conflicts_with = "json")]
    pub format: Option<OutputFormat>,
    /// Resume strictly after the last fully processed event cursor.
    #[arg(long, requires = "format")]
    pub cursor: Option<String>,
    #[arg(long, requires = "format")]
    pub max_events: Option<usize>,
    /// Bound total observation time; timeout does not cancel tasks.
    #[arg(long, requires = "format", value_parser = clap::value_parser!(u64).range(1..))]
    pub timeout_seconds: Option<u64>,
}
async fn emit(output: &mut Stdout, record: StreamRecordData) -> std::io::Result<()> {
    let mut line = serde_json::to_vec(&StreamRecord::new(record)).map_err(std::io::Error::other)?;
    line.push(b'\n');
    output.write_all(&line).await?;
    output.flush().await
}
fn write_exit(error: std::io::Error) -> i32 {
    if error.kind() == std::io::ErrorKind::BrokenPipe {
        0
    } else {
        eprintln!("Event stream output failed: {error}");
        8
    }
}
async fn fail(output: &mut Stdout, cursor: Option<String>, error: EditorError) -> i32 {
    let code = opencut_automation::exit_code(&error);
    match emit(output, StreamRecordData::Error { cursor, error }).await {
        Ok(()) => code,
        Err(error) => write_exit(error),
    }
}
pub async fn run(
    database: PathBuf,
    caller: opencut_automation::Caller,
    scope: EventScope,
    after: i64,
    limit: usize,
    options: Options,
) -> i32 {
    let mut output = tokio::io::stdout();
    let mut cursor = options.cursor.clone();
    if after != 0 || options.max_events == Some(0) {
        return fail(
            &mut output,
            cursor,
            EditorError::new(
                "INVALID_ARGUMENT",
                "NDJSON requires --cursor instead of numeric --after; max-events must be positive",
            ),
        )
        .await;
    }
    let request_id = uuid::Uuid::new_v4().to_string();
    let mut ready = false;
    let mut emitted = 0;
    loop {
        let path = database.clone();
        let input = EventChangesInput {
            scope: scope.clone(),
            cursor: cursor.clone(),
            limit,
        };
        let identity = caller.clone();
        let trace = request_id.clone();
        let read = tokio::task::spawn_blocking(move || {
            let mut app = opencut_host_runtime::workspace::Workspace::open_managed(path)?;
            app.set_caller(identity.clone());
            app.event_changes(input).map_err(|error| {
                use opencut_automation::denials::*;
                if !is_denial(&error) {
                    return error;
                }
                let failed = app
                    .record_denial(&Denial {
                        request_id: &trace,
                        source: opencut_automation::CallSource::Cli,
                        caller: Some(&identity),
                        operation: DeniedOperation::Stream,
                        reason: DenialReason::AuthorizationDenied,
                    })
                    .is_err();
                error.with_details(
                    serde_json::json!({"request_id":trace,"denial_audit_unavailable":failed}),
                )
            })
        })
        .await;
        let page = match read {
            Ok(Ok(page)) => page,
            Ok(Err(error)) => return fail(&mut output, cursor, error).await,
            Err(error) => {
                return fail(
                    &mut output,
                    cursor,
                    EditorError::new("INTERNAL_ERROR", error.to_string()),
                )
                .await;
            }
        };
        if !ready {
            if let Err(error) = emit(
                &mut output,
                StreamRecordData::Ready {
                    scope: scope.clone(),
                    cursor: cursor.clone(),
                },
            )
            .await
            {
                return write_exit(error);
            }
            ready = true;
        }
        let empty = page.events.is_empty();
        for change in page.events {
            let next = change.cursor.clone();
            if let Err(error) = emit(
                &mut output,
                StreamRecordData::Event {
                    cursor: change.cursor,
                    event: change.event,
                },
            )
            .await
            {
                return write_exit(error);
            }
            cursor = Some(next);
            emitted += 1;
            if options.max_events.is_some_and(|max| emitted >= max) {
                return match emit(
                    &mut output,
                    StreamRecordData::End {
                        cursor,
                        reason: StreamEnd::LimitReached,
                    },
                )
                .await
                {
                    Ok(()) => 0,
                    Err(e) => write_exit(e),
                };
            }
        }
        if empty {
            if !options.watch {
                return match emit(
                    &mut output,
                    StreamRecordData::End {
                        cursor,
                        reason: StreamEnd::CaughtUp,
                    },
                )
                .await
                {
                    Ok(()) => 0,
                    Err(e) => write_exit(e),
                };
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    }
}
