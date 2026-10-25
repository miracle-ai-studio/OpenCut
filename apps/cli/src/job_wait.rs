//! Interrupting an observation and cancelling durable work are separate actions.
use crate::connection::Connection;
use opencut_automation::Response;
use opencut_contracts::EditorError;
use serde_json::json;

async fn observe(
    source: Connection,
    job_id: &str,
    timeout_seconds: u64,
) -> Result<Response, EditorError> {
    use opencut_editor_application::JobState;
    use opencut_host_runtime::workspace::Workspace as EditorApplication;
    let started = std::time::Instant::now();
    loop {
        let source = source.clone();
        let id = job_id.to_owned();
        let job = tokio::task::spawn_blocking(move || match source {
            Connection::Local { database, caller } => {
                let mut app = EditorApplication::open_managed(database)?;
                app.set_caller(caller);
                app.job_view(&id)
            }
            remote => remote.data("opencut_job_view", json!({"job_id":id})),
        })
        .await
        .map_err(|error| EditorError::new("INTERNAL_ERROR", error.to_string()))??;
        let data = json!(job);
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
    source: Connection,
    job_id: &str,
    timeout: u64,
    cancel_on_interrupt: bool,
) -> Result<Response, EditorError> {
    tokio::select! {
        result = observe(source.clone(),job_id,timeout) => result,
        signal=tokio::signal::ctrl_c()=>{
            signal.map_err(|error| EditorError::new("INTERNAL_ERROR",error.to_string()))?;
            let cancellation = if cancel_on_interrupt {
                let source=source.clone();
                let id=job_id.to_owned();
                Some(tokio::task::spawn_blocking(move || {
                    let tool = "opencut_job_request_cancel";
                    source.call(tool, json!({"job_id":id}))
                }).await.map_err(|error|EditorError::new("INTERNAL_ERROR",error.to_string()))?)
            } else {None};
            Err(EditorError::new("USER_INTERRUPTED","Job observation was interrupted")
                .with_details(json!({"job_id":job_id,"cancellation_requested":cancel_on_interrupt,"cancellation":cancellation})))
        }
    }
}
