use opencut_sdk::{
    Client, Error, WaitOptions,
    api::{self, tools},
    job_kinds, job_result, media, project,
};
use serde_json::{Value, json};
use std::{
    env, fs,
    path::Path,
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

fn cli(binary: &str, database: &str, tool: &str, input: Value) -> Value {
    let path = format!("{database}.request.json");
    fs::write(&path, serde_json::to_vec(&input).unwrap()).unwrap();
    let result = Command::new(binary)
        .args([
            "--database",
            database,
            "--json",
            "call",
            tool,
            "--input",
            &path,
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice::<Value>(&result.stdout).unwrap()["data"].clone()
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    let [
        base,
        proxy,
        token_file,
        binary,
        database,
        fixture_file,
        cancelled_id,
        wave,
        report,
    ]: &[String; 9] = args.as_slice().try_into()?;
    let token = fs::read_to_string(token_file)?.trim().to_string();
    let client = Client::new(base, token.clone())?;
    let proxy_client = Client::new(proxy, token)?;
    let fixture: Value = serde_json::from_slice(&fs::read(fixture_file)?)?;
    let mut checks = vec![];
    let capabilities = client.call_envelope::<tools::Capabilities>(&api::EmptyInput {})?;
    assert!(!capabilities.request_id.is_empty());
    assert_eq!(
        capabilities.data.tools.len(),
        api::tools::tool_specs().len()
    );
    let bad_client = Client::new(base, "wrong-token")?;
    assert!(
        matches!(bad_client.call::<tools::ProjectList>(&project::project_catalog::LibraryInput {cursor:None,limit:1}), Err(Error::Api(failure)) if failure.status == 403 && failure.error.code == "NOT_AUTHORIZED")
    );
    checks.push("typed discovery and structured authentication failure");
    let create = serde_json::from_value(fixture["create"].clone())?;
    let created = client.call::<tools::ProjectCreate>(&create)?;
    assert_eq!(client.call::<tools::ProjectCreate>(&create)?, created);
    let mut request = fixture["request"].clone();
    request["expected_revision"] = json!(created.revision);
    request["operations"] = fixture["operations"].clone();
    request["project_id"] = json!(created.project_id);
    let edit: project::project_public::ApplyProject = serde_json::from_value(request)?;
    let initial =
        client.call::<tools::ProjectInspect>(&project::project_inspection::InspectProject {
            project_id: created.project_id.clone(),
            revision: Some(created.revision.clone()),
        })?;
    assert!(matches!(
        &initial,
        project::project_inspection::ProjectInspection::Editor { .. }
    ));
    client.call::<tools::EditPlan>(&edit)?;
    assert_eq!(
        cli(
            binary,
            database,
            "opencut_project_inspect",
            json!({"project_id": created.project_id})
        ),
        json!(initial)
    );
    checks
        .push("shared conformance fixture, idempotent creation and read-only plan verified by CLI");
    assert!(matches!(
        proxy_client.call::<tools::EditApply>(&edit),
        Err(Error::Transport(_))
    ));
    let committed = cli(
        binary,
        database,
        "opencut_project_inspect",
        json!({"project_id": created.project_id}),
    );
    assert_ne!(committed["snapshot"]["revision"], json!(created.revision));
    let applied = proxy_client.call::<tools::EditApply>(&edit)?;
    assert_eq!(json!(applied.revision), committed["snapshot"]["revision"]);
    assert_eq!(client.call::<tools::EditApply>(&edit)?, applied);
    checks.push("real lost response after commit; identical replay returns original transaction");
    let mut stale = edit.clone();
    stale.idempotency_key = "sdk-stale".into();
    stale.operations = serde_json::from_value(fixture["conflicting_operations"].clone())?;
    assert!(
        matches!(client.call::<tools::EditApply>(&stale), Err(Error::Api(failure)) if failure.error.code == fixture["expected_conflict"]["code"].as_str().unwrap() && failure.status == 409 && !failure.request_id.is_empty())
    );
    let mut reused = edit.clone();
    reused.operations = stale.operations;
    assert!(
        matches!(client.call::<tools::EditApply>(&reused), Err(Error::Api(failure)) if failure.error.code == "IDEMPOTENCY_KEY_REUSED")
    );
    assert_eq!(
        cli(
            binary,
            database,
            "opencut_project_inspect",
            json!({"project_id": created.project_id})
        ),
        committed
    );
    checks.push("revision and idempotency conflicts preserve current head");
    let event_input = api::event_stream::EventChangesInput {
        scope: api::event_stream::EventScope::Projects {
            project_id: Some(created.project_id.clone()),
        },
        cursor: None,
        limit: 100,
    };
    let events = client.call::<tools::EventChanges>(&event_input)?;
    assert_eq!(
        json!(events),
        cli(
            binary,
            database,
            "opencut_event_changes",
            json!(event_input)
        )
    );
    assert_eq!(
        events
            .events
            .iter()
            .filter(|entry| matches!(&entry.event,
        api::event_stream::EventData::Project(event) if event.revision == applied.revision))
            .count(),
        1
    );
    checks.push("event cursor and single commit agree with CLI");
    assert!(Path::new(wave).is_file());
    let accepted = client.call::<tools::JobSubmit>(&api::job_submission::SubmitJob::Probe {
        idempotency_key: "rust-sdk-probe".into(),
        input: media::ProbeRequest {
            input: wave.clone(),
        },
    })?;
    assert!(matches!(
        proxy_client.wait_for_job(
            &accepted.job_id,
            WaitOptions {
                timeout: Duration::from_millis(20),
                ..Default::default()
            }
        ),
        Err(Error::WaitTimeout { .. })
    ));
    let cancelled = AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(Duration::from_millis(20));
            cancelled.store(true, Ordering::Release);
        });
        assert!(matches!(
            proxy_client.wait_for_job(
                &accepted.job_id,
                WaitOptions {
                    cancelled: Some(&cancelled),
                    ..Default::default()
                }
            ),
            Err(Error::ObservationCancelled)
        ));
    });
    let completed = client.wait_for_job(&accepted.job_id, WaitOptions::default())?;
    assert!(!completed.cancel_requested);
    let metadata = job_result::<job_kinds::Probe>(&completed)?;
    assert!(
        metadata
            .streams
            .iter()
            .any(|s| s.codec_type == media::StreamKind::Audio)
    );
    assert!(job_result::<job_kinds::ProjectRender>(&completed).is_err());
    assert_eq!(
        json!(completed),
        cli(
            binary,
            database,
            "opencut_job_view",
            json!({"job_id":accepted.job_id})
        )
    );
    checks.push("real worker result, local timeout and observation cancellation never cancel the durable task");
    assert!(matches!(
        completed.result,
        Some(api::job_results::JobResultView::Probe(_))
    ));
    assert!(!serde_json::to_string(&completed)?.contains(database));
    let failed = client.call::<tools::JobSubmit>(&api::job_submission::SubmitJob::Probe {
        idempotency_key: "rust-sdk-missing".into(),
        input: media::ProbeRequest {
            input: format!("{database}.missing"),
        },
    })?;
    assert!(
        matches!(client.wait_for_job(&failed.job_id, WaitOptions::default()), Err(Error::JobFailed(record)) if record.failure.is_some())
    );
    assert_eq!(
        client.cancel_job(cancelled_id)?.state,
        api::JobState::Cancelled
    );
    assert!(matches!(
        client.wait_for_job(cancelled_id, WaitOptions::default()),
        Err(Error::JobCancelled(_))
    ));
    checks.push("failed and cancelled jobs retain terminal records");
    fs::write(
        report,
        serde_json::to_vec_pretty(
            &json!({"status":"passed","checks":checks,"transaction":applied,"snapshot":committed,"jobs":[accepted.job_id,failed.job_id,cancelled_id]}),
        )?,
    )?;
    Ok(())
}
