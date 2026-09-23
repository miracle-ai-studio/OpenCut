use std::{fs, path::Path, process::Command};

use serde_json::{Value, json};
use tempfile::TempDir;

fn call(root: &Path, args: &[&str]) -> (i32, Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_opencut"))
        .arg("--database")
        .arg(root.join("workspace.sqlite"))
        .arg("--json")
        .args(args)
        .output()
        .expect("run CLI");
    let value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "JSON output: {e}; stdout={}; stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    (output.status.code().unwrap(), value)
}

#[test]
fn cli_persists_project_and_reports_conflict_without_duplicate_state() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("project.json");
    fs::write(&input, json!({"id":"project-demo","name":"Demo","sequence_id":"sequence-main","width":1920,"height":1080,"fps":{"numerator":30,"denominator":1}}).to_string()).unwrap();
    let args = [
        "project",
        "create",
        "--input",
        input.to_str().unwrap(),
        "--idempotency-key",
        "create-demo",
    ];
    let (code, first) = call(dir.path(), &args);
    assert_eq!(code, 0, "{first}");
    let (_, repeat) = call(dir.path(), &args);
    assert_eq!(first["data"], repeat["data"]);
    let (_, readback) = call(dir.path(), &["project", "inspect", "project-demo"]);
    assert_eq!(first["data"], readback["data"]);
    let (_, projects) = call(dir.path(), &["project", "list"]);
    assert_eq!(projects["data"].as_array().unwrap().len(), 1);
    let (_, events) = call(dir.path(), &["events"]);
    assert_eq!(events["data"].as_array().unwrap().len(), 1);

    let transaction = dir.path().join("transaction.json");
    fs::write(&transaction,json!({"expected_revision":"obsolete","idempotency_key":"edit-1","label":"stale edit","operations":[]}).to_string()).unwrap();
    let (code, error) = call(
        dir.path(),
        &[
            "edit",
            "apply",
            "--project",
            "project-demo",
            "--input",
            transaction.to_str().unwrap(),
        ],
    );
    assert_eq!(code, 5, "{error}");
    assert_eq!(error["error"]["code"], "REVISION_CONFLICT");
    let (_, after) = call(dir.path(), &["project", "inspect", "project-demo"]);
    assert_eq!(first["data"], after["data"]);
}

#[test]
fn discovery_is_schema_backed_and_does_not_create_database() {
    let dir = TempDir::new().unwrap();
    let (code, result) = call(dir.path(), &["system", "schema"]);
    assert_eq!(code, 0);
    let tools = result["data"].as_array().unwrap();
    let apply = tools
        .iter()
        .find(|t| t["name"] == "opencut_edit_apply")
        .unwrap();
    assert!(apply["input_schema"]["$defs"]["TransactionRequest"].is_object());
    assert_eq!(apply["input_schema"]["additionalProperties"], false);
    assert!(
        apply["output_schema"]["$defs"]["TransactionResult"]["properties"]["revision"].is_object()
    );
    let inspect = tools
        .iter()
        .find(|t| t["name"] == "opencut_project_inspect")
        .unwrap();
    assert!(inspect["output_schema"]["$defs"]["ProjectSnapshot"].is_object());
    assert!(!dir.path().join("workspace.sqlite").exists());
}

#[test]
fn argument_errors_follow_json_contract_without_database_side_effects() {
    let dir = TempDir::new().unwrap();
    for args in [
        vec!["project", "create"],
        vec!["missing-command"],
        vec!["events", "--limit", "invalid"],
    ] {
        let (code, result) = call(dir.path(), &args);
        assert_eq!(code, 2);
        assert_eq!(result["error"]["code"], "INVALID_ARGUMENT");
        assert!(result["data"].is_null());
    }
    assert!(!dir.path().join("workspace.sqlite").exists());
}

#[test]
fn malformed_json_has_machine_readable_error_and_no_database_side_effect() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("bad.json");
    fs::write(&input, "{broken}").unwrap();
    let (code, result) = call(
        dir.path(),
        &[
            "project",
            "create",
            "--input",
            input.to_str().unwrap(),
            "--idempotency-key",
            "bad",
        ],
    );
    assert_eq!(code, 2);
    assert_eq!(result["error"]["code"], "INVALID_ARGUMENT");
    assert!(!dir.path().join("workspace.sqlite").exists());
}

#[test]
fn wait_timeout_preserves_queued_job_and_cancelled_wait_exits_nonzero() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("job.json");
    fs::write(
        &input,
        json!({"kind":"media.probe","idempotency_key":"probe","input":{"input":"pending.mp4"}})
            .to_string(),
    )
    .unwrap();
    let (code, result) = call(
        dir.path(),
        &["job", "start", "--input", input.to_str().unwrap()],
    );
    assert_eq!(code, 0, "{result}");
    let id = result["data"]["job_id"].as_str().unwrap();
    let (code, timeout) = call(dir.path(), &["job", "wait", id, "--timeout-seconds", "0"]);
    assert_eq!(code, 124); // Design §15.2: timeout is distinct from job failure/cancel.
    assert_eq!(timeout["error"]["code"], "JOB_WAIT_TIMEOUT");
    let (_, queued) = call(dir.path(), &["job", "get", id]);
    assert_eq!(queued["data"]["state"], "queued");
    let (code, _) = call(dir.path(), &["job", "cancel", id]);
    assert_eq!(code, 0);
    let (code, cancelled) = call(dir.path(), &["job", "wait", id, "--timeout-seconds", "0"]);
    assert_eq!(code, 7);
    assert_eq!(cancelled["error"]["code"], "JOB_CANCELLED");
}
