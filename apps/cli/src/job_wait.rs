//! Interrupting an observation and cancelling durable work are separate actions.
use opencut_automation::Response;
use opencut_contracts::EditorError;
use serde_json::json;
use std::path::Path;

async fn observe(
    database: &Path,
    job_id: &str,
    timeout_seconds: u64,
    caller: opencut_automation::Caller,
) -> Result<Response, EditorError> {
    use opencut_sqlite_store::{EditorApplication, JobState};
    let started = std::time::Instant::now();
    loop {
        let path = database.to_path_buf();
        let id = job_id.to_owned();
        let identity = caller.clone();
        let job = tokio::task::spawn_blocking(move || {
            let mut app = EditorApplication::open_managed(path)?;
            app.set_caller(identity);
            app.get_job(&id)
        })
        .await
        .map_err(|error| EditorError::new("INTERNAL_ERROR", error.to_string()))??;
        // Owner keeps the legacy shape; scoped observers receive the safe public view.
        let data = if caller.is_owner() {
            json!(job)
        } else {
            json!(opencut_editor_application::job_views::project(job.clone()))
        };
        match job.state {
            JobState::Succeeded => return Ok(opencut_automation::response(Ok(data))),
            JobState::Failed | JobState::Cancelled => {
                let code = if job.state == JobState::Failed {
                    "JOB_FAILED"
                } else {
                    "JOB_CANCELLED"
                };
                return Err(EditorError::new(code, "Task did not complete successfully")
                    .with_details(json!({"job":data})));
            }
            _ => {}
        }
        if started.elapsed() >= std::time::Duration::from_secs(timeout_seconds) {
            return Err(EditorError::new(
                "JOB_WAIT_TIMEOUT",
                "Waiting timed out; the persistent task is still available",
            )
            .with_details(json!({"job_id":job_id,"state":job.state,"retryable":true})));
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
}
pub async fn run(
    database: &Path,
    job_id: &str,
    timeout: u64,
    cancel_on_interrupt: bool,
    caller: opencut_automation::Caller,
) -> Result<Response, EditorError> {
    tokio::select! {
        result = observe(database,job_id,timeout,caller.clone()) => result,
        signal=tokio::signal::ctrl_c()=>{
            signal.map_err(|error| EditorError::new("INTERNAL_ERROR",error.to_string()))?;
            let cancellation = if cancel_on_interrupt {
                let path=database.to_path_buf();
                let id=job_id.to_owned();
                let identity = caller.clone();
                Some(tokio::task::spawn_blocking(move || {
                    let tool = if identity.is_owner() {"opencut_job_cancel"} else {"opencut_job_request_cancel"};
                    opencut_automation::Automation::open(path).map(|app|app.with_source(opencut_automation::CallSource::Cli).with_caller(identity).call(tool, json!({"job_id":id})))
                }).await.map_err(|error|EditorError::new("INTERNAL_ERROR",error.to_string()))?
                    .unwrap_or_else(|error|opencut_automation::response(Err(error))))
            } else {None};
            Err(EditorError::new("USER_INTERRUPTED","Job observation was interrupted")
                .with_details(json!({"job_id":job_id,"cancellation_requested":cancel_on_interrupt,"cancellation":cancellation})))
        }
    }
}
