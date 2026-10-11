use serde_json::{Value, json};
use std::{
    fs,
    net::TcpListener,
    path::Path,
    process::Command,
    thread,
    time::{Duration, Instant},
};
use tempfile::TempDir;

struct Service(TempDir);
impl Service {
    fn new() -> Self {
        Self(TempDir::new().unwrap())
    }
    fn directory(&self) -> std::path::PathBuf {
        self.0.path().join("service")
    }
    fn call(&self, args: &[&str]) -> (i32, Value) {
        let output = Command::new(env!("CARGO_BIN_EXE_opencut"))
            .arg("--json")
            .arg("--database")
            .arg(self.0.path().join("workspace.sqlite"))
            .arg("server")
            .arg("--directory")
            .arg(self.directory())
            .args(args)
            .output()
            .unwrap();
        let value = serde_json::from_slice(&output.stdout).unwrap_or_else(|_| {
            panic!(
                "stdout={} stderr={}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )
        });
        (output.status.code().unwrap(), value)
    }
    fn start(&self) -> Value {
        let (code, result) = self.call(&["start", "--listen", "127.0.0.1:0"]);
        assert_eq!(code, 0, "{result}");
        assert_eq!(result["data"]["ready"], true);
        result["data"].clone()
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        let _ = self.call(&["stop"]);
    }
}
fn client(service: &Service, state: &Value) -> opencut_sdk::Client {
    opencut_sdk::Client::new(
        state["endpoint"].as_str().unwrap(),
        fs::read_to_string(service.directory().join("api-token")).unwrap(),
    )
    .unwrap()
}

#[test]
fn detached_lifecycle_is_idempotent_authenticated_and_restartable() {
    let service = Service::new();
    let (code, state) = service.call(&["status"]);
    assert_eq!(code, 1);
    assert_eq!(state["data"]["state"], "stopped");
    assert!(!service.directory().exists());
    let first = service.start(); // The launcher has exited, including its stdout/stderr pipes.
    assert_eq!(service.start()["instance"], first["instance"]);
    let (_, status) = service.call(&["status"]);
    assert_eq!(status["data"]["ready"], true);
    let bad = opencut_sdk::Client::new(first["endpoint"].as_str().unwrap(), "wrong-token").unwrap();
    assert!(
        bad.call::<opencut_sdk::api::tools::Capabilities>(&opencut_sdk::api::EmptyInput {})
            .is_err()
    );
    let token_file = service.directory().join("api-token");
    let token = fs::read(&token_file).unwrap();
    fs::write(&token_file, "incorrect-token-for-status-probe-only").unwrap();
    let (code, status) = service.call(&["status"]);
    assert_eq!(code, 1);
    assert_eq!(status["data"]["state"], "unhealthy");
    fs::write(token_file, token).unwrap();
    assert_eq!(service.call(&["stop"]).0, 0);
    assert_eq!(service.call(&["stop"]).0, 0);
    assert_eq!(service.call(&["status"]).0, 1);
    let second = service.start();
    assert_ne!(first["instance"], second["instance"]);
    // An old stop request cannot shut down the replacement process.
    fs::write(
        service.directory().join("stop-request"),
        first["instance"].as_str().unwrap(),
    )
    .unwrap();
    thread::sleep(Duration::from_millis(250));
    assert_eq!(service.call(&["status"]).0, 0);
}

#[test]
fn occupied_port_and_changed_configuration_never_report_success() {
    let service = Service::new();
    let occupied = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = occupied.local_addr().unwrap().to_string();
    let (code, result) = service.call(&["start", "--listen", &addr]);
    assert_ne!(code, 0, "{result}");
    assert_eq!(service.call(&["status"]).0, 1);
    service.start();
    assert_ne!(service.call(&["start", "--listen", &addr]).0, 0);
    assert_eq!(service.call(&["status"]).0, 0);
}

#[test]
fn stale_pid_is_not_liveness_and_does_not_receive_stop_signals() {
    let service = Service::new();
    fs::create_dir_all(service.directory()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(service.directory(), fs::Permissions::from_mode(0o700)).unwrap();
    }
    fs::write(
        service.directory().join("state.json"),
        json!({"pid":std::process::id()}).to_string(),
    )
    .unwrap();
    assert_eq!(service.call(&["status"]).0, 1);
    assert_eq!(service.call(&["stop"]).0, 0);
    service.start();
}

#[cfg(unix)]
#[test]
fn process_crash_releases_lock_and_allows_replacement() {
    let service = Service::new();
    let first = service.start();
    assert!(
        Command::new("kill")
            .args(["-KILL", &first["pid"].to_string()])
            .status()
            .unwrap()
            .success()
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while service.call(&["status"]).0 == 0 {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(50));
    }
    assert_ne!(service.start()["instance"], first["instance"]);
}

#[test]
fn background_worker_executes_job_after_submission_client_exits() {
    let service = Service::new();
    let state = service.start();
    let sdk = client(&service, &state);
    // Missing media deterministically reaches a terminal failure; no FFmpeg dependency in this test.
    let input_path = service.0.path().join("job.json");
    fs::write(&input_path, json!({"kind":"media.probe","idempotency_key":"detached-job","input":{"input":service.0.path().join("missing.mp4")}}).to_string()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_opencut"))
        .args([
            "--json",
            "--endpoint",
            state["endpoint"].as_str().unwrap(),
            "--credential-file",
        ])
        .arg(service.directory().join("api-token"))
        .args(["call", "opencut_job_start", "--input"])
        .arg(&input_path)
        .output()
        .unwrap();
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(output.status.success(), "{result}");
    let id = result["data"]["job_id"].as_str().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let job = sdk
            .call_registered("opencut_job_get", &json!({"job_id":id}))
            .unwrap()
            .data;
        if job["state"] == "failed" {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "worker did not complete job: {job}"
        );
        thread::sleep(Duration::from_millis(100));
    }
    assert_eq!(service.call(&["status"]).0, 0);
}

#[test]
fn remote_and_restricted_credentials_cannot_manage_local_service() {
    let service = Service::new();
    let output = Command::new(env!("CARGO_BIN_EXE_opencut"))
        .arg("--json")
        .arg("--credential-file")
        .arg(Path::new("unused-token"))
        .arg("server")
        .arg("--directory")
        .arg(service.directory())
        .arg("start")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!service.directory().exists());
    assert_ne!(service.call(&["start", "--listen", "0.0.0.0:8788"]).0, 0);
}

#[test]
fn static_web_host_preserves_api_authentication_and_rejects_path_and_host_escape() {
    use std::io::{Read, Write};
    let service = Service::new();
    let root = service.0.path().join("web");
    fs::create_dir_all(root.join("assets")).unwrap();
    fs::write(
        root.join("index.html"),
        "<title>OpenCut test editor</title>",
    )
    .unwrap();
    fs::write(root.join("assets/app.js"), "export const app = true;").unwrap();
    fs::write(service.0.path().join("private-token"), "must-not-be-served").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        service.0.path().join("private-token"),
        root.join("assets/escape.js"),
    )
    .unwrap();
    let (code, state) = service.call(&[
        "start",
        "--listen",
        "127.0.0.1:0",
        "--web-root",
        root.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{state}");
    let endpoint = state["data"]["endpoint"].as_str().unwrap();
    let address = endpoint.strip_prefix("http://").unwrap();
    let request = |method: &str, path: &str, host: &str, headers: &str| {
        let mut stream = std::net::TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        write!(
            stream,
            "{method} {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n{headers}\r\n"
        )
        .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    };
    let page = request("GET", "/editor", address, "");
    assert!(page.starts_with("HTTP/1.1 200"), "{page}");
    assert!(page.contains("OpenCut test editor"));
    for route in [
        "/",
        "/projects",
        "/editor/speech",
        "/editor/%E5%89%AA%E8%BE%91%20%2F%20100%25",
        "/brand",
        "/roadmap",
        "/sponsors",
        "/privacy",
        "/terms",
        "/contributors",
        "/blog",
        "/blog/first-post",
        "/changelog",
        "/changelog/0.3.0",
    ] {
        let response = request("GET", route, address, "");
        assert!(response.starts_with("HTTP/1.1 200"), "{route}: {response}");
        assert!(response.contains("OpenCut test editor"));
        assert!(request("HEAD", route, address, "").starts_with("HTTP/1.1 200"));
        assert!(
            request("POST", route, address, "Content-Length: 0\r\n").starts_with("HTTP/1.1 405")
        );
    }
    for route in [
        "/editor/../private-token",
        "/editor/a/b",
        "/editor/%",
        "/editor/%ff",
        "/editor/%00",
        "/editor/%2e%2e",
        "/blog/",
        "/blog/../private-token",
        "/blog/%2fprivate-token",
        "/blog/a/b",
        "/changelog/../private-token",
        "/changelog/no-release",
    ] {
        assert!(
            request("GET", route, address, "").starts_with("HTTP/1.1 404"),
            "{route}"
        );
    }
    let head = request("HEAD", "/editor", address, "");
    assert!(head.starts_with("HTTP/1.1 200"));
    assert!(!head.contains("<title>"));
    assert!(request("GET", "/v1/capabilities", address, "").starts_with("HTTP/1.1 403"));
    assert!(request("GET", "/editor", "attacker.example", "").starts_with("HTTP/1.1 403"));
    for path in [
        "/private-token",
        "/assets/../private-token",
        "/assets/%2e%2e/private-token",
        "/assets/escape.js",
        "/v1/missing",
    ] {
        let response = request("GET", path, address, "");
        assert!(response.starts_with("HTTP/1.1 404"), "{path}: {response}");
        assert!(!response.contains("must-not-be-served"));
    }
    assert!(
        request("POST", "/editor", address, "Content-Length: 0\r\n").starts_with("HTTP/1.1 405")
    );
    let session = request(
        "POST",
        "/app/session",
        address,
        &format!(
            "Origin: http://{address}\r\nSec-Fetch-Site: same-origin\r\nX-OpenCut-App: web\r\nContent-Length: 0\r\n"
        ),
    );
    assert!(session.starts_with("HTTP/1.1 204"), "{session}");
    let cookie_line = session
        .lines()
        .find(|line| line.to_ascii_lowercase().starts_with("set-cookie:"))
        .unwrap();
    assert!(cookie_line.contains("HttpOnly") && cookie_line.contains("SameSite=Strict"));
    let cookie = cookie_line
        .split_once(':')
        .unwrap()
        .1
        .trim()
        .split(';')
        .next()
        .unwrap();
    assert!(
        request(
            "GET",
            "/v1/capabilities",
            address,
            &format!("Cookie: {cookie}\r\nSec-Fetch-Site: same-origin\r\n")
        )
        .starts_with("HTTP/1.1 200")
    );
    assert!(
        request(
            "GET",
            "/v1/capabilities",
            address,
            &format!("Cookie: {cookie}\r\nSec-Fetch-Site: cross-site\r\n")
        )
        .starts_with("HTTP/1.1 403")
    );
    for extra in [
        "Content-Length: 0\r\n".to_string(),
        "Origin: https://attacker.example\r\nX-OpenCut-App: web\r\nContent-Length: 0\r\n"
            .to_string(),
        format!("Origin: http://{address}\r\nContent-Length: 0\r\n"),
    ] {
        assert!(request("POST", "/app/session", address, &extra).starts_with("HTTP/1.1 403"));
    }
    let token = fs::read_to_string(service.directory().join("api-token")).unwrap();
    let localhost = format!("localhost:{}", address.rsplit(':').next().unwrap());
    let same_origin = request(
        "GET",
        "/v1/capabilities",
        &localhost,
        &format!(
            "Authorization: Bearer {}\r\nOrigin: http://{localhost}\r\n",
            token.trim()
        ),
    );
    assert!(same_origin.starts_with("HTTP/1.1 200"), "{same_origin}");
    assert_ne!(service.call(&["start", "--listen", "127.0.0.1:0"]).0, 0);
}

#[test]
fn starts_from_service_configuration_without_web_arguments() {
    let service = Service::new();
    let root = service.0.path().join("web");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("index.html"), "<title>configured</title>").unwrap();
    let config = service.0.path().join("service-config.json");
    fs::write(
        &config,
        serde_json::to_vec(&json!({"listen":"127.0.0.1:0","web_root":"web"})).unwrap(),
    )
    .unwrap();
    let (code, result) = service.call(&["start", "--config", config.to_str().unwrap()]);
    assert_eq!(code, 0, "{result}");
    assert!(
        result["data"]["web_url"]
            .as_str()
            .unwrap()
            .ends_with("/editor")
    );
    assert_eq!(
        service
            .call(&["start", "--config", config.to_str().unwrap()])
            .1["data"]["instance"],
        result["data"]["instance"]
    );
}

#[test]
fn transcription_config_resolves_model_and_preserves_idempotent_service_configuration() {
    let service = Service::new();
    let web = service.0.path().join("web");
    fs::create_dir(&web).unwrap();
    fs::write(web.join("index.html"), "editor").unwrap();
    let model = service.0.path().join("model.bin");
    fs::write(&model, b"host model fixture").unwrap();
    let config = service.0.path().join("config.json");
    fs::write(
        &config,
        serde_json::to_vec(&json!({"listen":"127.0.0.1:0","web_root":"web",
        "transcription":{"model":"model.bin","executable":"whisper-cli"}}))
        .unwrap(),
    )
    .unwrap();
    let (code, state) = service.call(&["start", "--config", config.to_str().unwrap()]);
    assert_eq!(code, 0, "{state}");
    assert_eq!(state["data"]["ready"], true);
    let stored: Value =
        serde_json::from_slice(&fs::read(service.directory().join("state.json")).unwrap()).unwrap();
    assert_eq!(stored["transcription"]["model"], json!(model));
    let (code, replayed) = service.call(&["start", "--config", config.to_str().unwrap()]);
    assert_eq!(code, 0, "{replayed}");
    assert_eq!(state["data"]["instance"], replayed["data"]["instance"]);
    fs::write(
        &config,
        serde_json::to_vec(&json!({"listen":"127.0.0.1:0","web_root":"web",
        "transcription":{"model":"model.bin","executable":"other-whisper"}}))
        .unwrap(),
    )
    .unwrap();
    let (code, rejected) = service.call(&["start", "--config", config.to_str().unwrap()]);
    assert_ne!(code, 0);
    assert_eq!(rejected["error"]["code"], "SERVICE_ERROR");
}
