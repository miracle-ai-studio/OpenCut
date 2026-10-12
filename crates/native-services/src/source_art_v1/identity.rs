use super::*;
#[cfg(unix)]
pub fn resolve_identity(
    node: &Path,
    runner: &Path,
    chromium: &Path,
) -> Result<SourceArtIdentityV1, EditorError> {
    use crate::classic_audio::lifecycle::ProcessGuard;
    use std::{
        io::Read,
        os::unix::process::CommandExt,
        process::{Command, Stdio},
        sync::{
            Arc,
            atomic::{AtomicBool, AtomicU32},
        },
        time::Instant,
    };
    for path in [node, runner, chromium] {
        trusted_file(path)?;
    }
    let child = Command::new(node)
        .arg(runner)
        .arg("--identity")
        .arg(chromium)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|_| {
            error(
                "CAPABILITY_UNAVAILABLE",
                "Source-art identity process could not start",
            )
        })?;
    let mut guard = ProcessGuard {
        child,
        browser_pid: Arc::new(AtomicU32::new(0)),
        done: Arc::new(AtomicBool::new(false)),
    };
    let stdout = guard
        .child
        .stdout
        .take()
        .ok_or_else(|| protocol("Missing identity output"))?;
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdout.take(8193).read_to_end(&mut bytes).map(|_| bytes);
        let _ = tx.send(result);
    });
    let started = Instant::now();
    let result = (|| {
        loop {
            if let Some(status) = guard
                .child
                .try_wait()
                .map_err(|_| protocol("Could not observe identity process exit"))?
            {
                if !status.success() {
                    return Err(error(
                        "CAPABILITY_UNAVAILABLE",
                        "Source-art identity resolution failed",
                    ));
                }
                break;
            }
            if started.elapsed() >= Duration::from_secs(30) {
                return Err(error(
                    "SOURCE_ART_TIME_LIMIT",
                    "Source-art identity resolution exceeded deadline",
                ));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let bytes = rx
            .recv_timeout(Duration::from_secs(1))
            .map_err(|_| protocol("Identity output did not close"))?
            .map_err(|_| protocol("Could not read identity output"))?;
        if bytes.len() > 8192 {
            return Err(protocol("Identity output exceeds budget"));
        }
        let identity = serde_json::from_slice(&bytes)
            .map_err(|_| protocol("Invalid typed source-art identity"))?;
        validate_identity(&identity)?;
        Ok(identity)
    })();
    guard.terminate();
    let _ = reader.join();
    result
}
#[cfg(not(unix))]
pub fn resolve_identity(
    _node: &Path,
    _runner: &Path,
    _chromium: &Path,
) -> Result<SourceArtIdentityV1, EditorError> {
    Err(error(
        "CAPABILITY_UNAVAILABLE",
        "Source-art process isolation requires Unix",
    ))
}
