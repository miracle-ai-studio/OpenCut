//! Bounded process transport and teardown shared by fixed browser adapters.
use crate::classic_audio::{lifecycle::*, transport::PACKET_BYTES};
use opencut_contracts::EditorError;
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Read},
    path::Path,
    process::{ChildStdin, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};
// Shared host launch is capped at 10s. Failed cleanup leaves bounded time for
// late started packets, close/kill and actual exit; the watchdog uses the same
// grace and cannot kill the supervisor at the old 5s launch midpoint.
const FAILURE_CLEANUP_GRACE: Duration = Duration::from_secs(15);
pub(crate) struct BrowserProcess {
    pub(crate) guard: ProcessGuard,
    pub(crate) input: Option<ChildStdin>,
    receiver: mpsc::Receiver<Result<Value, EditorError>>,
    reader: Option<JoinHandle<()>>,
    diagnostics: Option<JoinHandle<()>>,
    watchdog: Option<JoinHandle<()>>,
    watchdog_active: Arc<Mutex<bool>>,
}
impl BrowserProcess {
    #[cfg(unix)]
    pub(crate) fn start(
        node: &Path,
        runner: &Path,
        deadline: Duration,
    ) -> Result<Self, EditorError> {
        use std::os::unix::process::CommandExt;
        let child = Command::new(node)
            .arg(runner)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .spawn()
            .map_err(|_| {
                err(
                    "CAPABILITY_UNAVAILABLE",
                    "Configured browser runner could not start",
                )
            })?;
        let mut guard = ProcessGuard {
            child,
            browser_pid: Arc::new(AtomicU32::new(0)),
            done: Arc::new(AtomicBool::new(false)),
        };
        let input = guard
            .child
            .stdin
            .take()
            .ok_or_else(|| err("BROWSER_HOST_FAILED", "Missing runner input"))?;
        let stdout = guard
            .child
            .stdout
            .take()
            .ok_or_else(|| err("BROWSER_HOST_FAILED", "Missing runner output"))?;
        let stderr = guard
            .child
            .stderr
            .take()
            .ok_or_else(|| err("BROWSER_HOST_FAILED", "Missing runner diagnostics"))?;
        let (sender, receiver) = mpsc::sync_channel(4);
        let reader = std::thread::spawn(move || {
            let mut source = BufReader::new(stdout);
            loop {
                let mut line = Vec::new();
                let result = source
                    .by_ref()
                    .take((PACKET_BYTES + 1) as u64)
                    .read_until(b'\n', &mut line);
                let message = match result {
                    Ok(0) => break,
                    Ok(_) if line.len() <= PACKET_BYTES && line.last() == Some(&b'\n') => {
                        serde_json::from_slice(&line)
                            .map_err(|_| err("BROWSER_PROTOCOL_ERROR", "Malformed runner packet"))
                    }
                    _ => Err(err(
                        "BROWSER_PROTOCOL_ERROR",
                        "Oversized or unterminated runner packet",
                    )),
                };
                let failed = message.is_err();
                if sender.send(message).is_err() || failed {
                    break;
                }
            }
        });
        let diagnostics = std::thread::spawn(move || {
            let mut source = stderr;
            let mut bytes = [0; 4096];
            let mut count = 0usize;
            while let Ok(n) = source.read(&mut bytes) {
                if n == 0 {
                    break;
                }
                count = count.saturating_add(n);
                if count > 65536 {
                    break;
                }
            }
        });
        let pid = guard.child.id();
        let browser_pid = Arc::clone(&guard.browser_pid);
        let done = Arc::clone(&guard.done);
        let watchdog_active = Arc::new(Mutex::new(true));
        let active = Arc::clone(&watchdog_active);
        let watchdog = std::thread::spawn(move || {
            let start = Instant::now();
            while !done.load(Ordering::Acquire) {
                if start.elapsed() >= deadline + FAILURE_CLEANUP_GRACE {
                    // Signal and deactivation share one lock. The worker must
                    // deactivate before reaping, so a reused PID is never
                    // signalled by this watchdog after finish begins.
                    let active = active.lock().unwrap_or_else(|e| e.into_inner());
                    if *active {
                        kill_group(browser_pid.load(Ordering::Acquire));
                        kill_group(pid);
                    }
                    break;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
        });
        Ok(Self {
            guard,
            input: Some(input),
            receiver,
            reader: Some(reader),
            diagnostics: Some(diagnostics),
            watchdog: Some(watchdog),
            watchdog_active,
        })
    }
    pub(crate) fn receive(&self) -> Result<Option<Value>, EditorError> {
        self.receive_timeout(Duration::from_millis(50))
    }
    pub(crate) fn receive_timeout(&self, timeout: Duration) -> Result<Option<Value>, EditorError> {
        receive_packet(&self.receiver, timeout)
    }
    fn stop_watchdog(&self) {
        let mut active = self
            .watchdog_active
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *active = false;
        self.guard.done.store(true, Ordering::Release);
    }
    pub(crate) fn finish(mut self, successful: bool, execution: &str) -> Result<(), EditorError> {
        // The worker owns bounded teardown: successful execution gets 5s;
        // failed startup gets the shared 15s launch/close grace. No wait holds
        // the watchdog mutex.
        self.stop_watchdog();
        self.input.take();
        let limit = if successful {
            Duration::from_secs(5)
        } else {
            FAILURE_CLEANUP_GRACE
        };
        let start = Instant::now();
        let mut exited = None;
        let mut cleanup_error = None;
        let mut output_closed = false;
        loop {
            // Continuously drain the four-slot queue while the runner closes.
            // A late started packet must reach the native group guard even
            // after the application loop stopped on cancellation or failure.
            for _ in 0..4 {
                match self.receiver.try_recv() {
                    Ok(message) => {
                        let result = message.and_then(|value| {
                            register_cleanup_packet(
                                &value,
                                execution,
                                self.guard.child.id(),
                                &self.guard.browser_pid,
                            )
                        });
                        if let Err(error) = result {
                            cleanup_error.get_or_insert(error);
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        output_closed = true;
                        break;
                    }
                }
            }
            if exited.is_none() {
                match self.guard.child.try_wait() {
                    Ok(status) => exited = status,
                    Err(_) => {
                        cleanup_error.get_or_insert_with(|| {
                            err("BROWSER_HOST_FAILED", "Could not observe runner exit")
                        });
                    }
                }
            }
            if exited.is_some() && output_closed {
                break;
            }
            if start.elapsed() >= limit {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let browser = self.guard.browser_pid.load(Ordering::Acquire);
        let browser_gone = !group_exists(browser);
        let forced_unknown_startup = exited.is_none() && browser == 0;
        if exited.is_none() || !successful || !browser_gone {
            self.guard.terminate();
        }
        let acknowledged = wait_group_gone(browser, Duration::from_secs(1));
        self.guard.done.store(true, Ordering::Release);
        // Drain the bounded reader queue before joining, including malformed
        // extra output after complete. It can otherwise be blocked on send.
        while self.receiver.try_recv().is_ok() {}
        if let Some(thread) = self.watchdog.take() {
            let _ = thread.join();
        }
        // Receiver remains live until self drops. A sender may refill it while
        // draining; replace/drop it now so the reader is guaranteed to unblock.
        let (_, empty) = mpsc::channel();
        let old = std::mem::replace(&mut self.receiver, empty);
        drop(old);
        if let Some(thread) = self.reader.take() {
            let _ = thread.join();
        }
        if let Some(thread) = self.diagnostics.take() {
            let _ = thread.join();
        }
        if forced_unknown_startup {
            return Err(err(
                "BROWSER_TERMINATION_FAILED",
                "Forced startup termination lacks an observed browser group acknowledgement",
            ));
        }
        if let Some(error) = cleanup_error {
            return Err(error);
        }
        if !acknowledged {
            return Err(err(
                "BROWSER_TERMINATION_FAILED",
                "Browser group termination was not acknowledged",
            ));
        }
        if successful && (exited.is_none_or(|s| !s.success()) || !browser_gone) {
            return Err(err(
                "BROWSER_HOST_FAILED",
                "Runner or browser remained active after terminal acknowledgement",
            ));
        }
        Ok(())
    }
}
impl Drop for BrowserProcess {
    fn drop(&mut self) {
        self.stop_watchdog();
        self.input.take();
        if self.guard.child.try_wait().ok().flatten().is_none() {
            self.guard.terminate();
        }
    }
}
fn err(code: &str, message: &str) -> EditorError {
    EditorError::new(code, message)
}

fn receive_packet(
    receiver: &mpsc::Receiver<Result<Value, EditorError>>,
    timeout: Duration,
) -> Result<Option<Value>, EditorError> {
    match receiver.recv_timeout(timeout) {
        Ok(value) => value.map(Some),
        Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
        Err(_) => Err(err(
            "BROWSER_HOST_FAILED",
            "Runner exited without terminal acknowledgement",
        )),
    }
}

fn register_cleanup_packet(
    value: &Value,
    execution: &str,
    node_pid: u32,
    browser_pid: &AtomicU32,
) -> Result<(), EditorError> {
    if value.get("protocolVersion").and_then(Value::as_u64) != Some(1)
        || value.get("executionId").and_then(Value::as_str) != Some(execution)
        || !matches!(
            value.get("type").and_then(Value::as_str),
            Some(
                "started"
                    | "ready"
                    | "request"
                    | "pcm"
                    | "rgba"
                    | "art_bytes"
                    | "complete"
                    | "failed"
                    | "cancelled"
            )
        )
    {
        return Err(err(
            "BROWSER_PROTOCOL_ERROR",
            "Late cleanup packet changed its fixed execution envelope",
        ));
    }
    if value.get("type").and_then(Value::as_str) != Some("started") {
        return Ok(());
    }
    let pid = value
        .get("browserPid")
        .and_then(Value::as_u64)
        .filter(|pid| *pid > 0 && *pid <= i32::MAX as u64 && *pid != u64::from(node_pid))
        .ok_or_else(|| {
            err(
                "BROWSER_PROTOCOL_ERROR",
                "Invalid late browser process identity",
            )
        })? as u32;
    let previous = browser_pid.load(Ordering::Acquire);
    if previous != 0 && previous != pid {
        return Err(err(
            "BROWSER_PROTOCOL_ERROR",
            "Late browser process identity changed",
        ));
    }
    browser_pid.store(pid, Ordering::Release);
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn idle_poll_preserves_packets_errors_and_disconnect_without_waiting_for_a_packet() {
        let (sender, receiver) = mpsc::channel();
        // An empty live queue is normal between preview commands. Zero timeout
        // must not introduce the blocking receive's 50ms into each audio block.
        assert!(receive_packet(&receiver, Duration::ZERO).unwrap().is_none());
        let packet = json!({"protocolVersion":1,"executionId":"fixed","type":"failed"});
        sender.send(Ok(packet.clone())).unwrap();
        assert_eq!(
            receive_packet(&receiver, Duration::ZERO).unwrap(),
            Some(packet)
        );
        sender
            .send(Err(err(
                "BROWSER_PROTOCOL_ERROR",
                "Malformed runner packet",
            )))
            .unwrap();
        assert_eq!(
            receive_packet(&receiver, Duration::ZERO).unwrap_err().code,
            "BROWSER_PROTOCOL_ERROR"
        );
        drop(sender);
        assert_eq!(
            receive_packet(&receiver, Duration::ZERO).unwrap_err().code,
            "BROWSER_HOST_FAILED"
        );
    }
    #[test]
    fn late_source_art_bytes_keep_the_fixed_cleanup_envelope() {
        let pid = AtomicU32::new(12345);
        let packet =
            json!({"protocolVersion":1,"executionId":"fixed","type":"art_bytes","bytes_hex":"00"});
        register_cleanup_packet(&packet, "fixed", 12344, &pid).unwrap();
        assert!(register_cleanup_packet(&packet, "other", 12344, &pid).is_err());
        assert_eq!(pid.load(Ordering::Acquire), 12345);
    }
    #[test]
    fn late_startup_pid_is_scoped_and_stable_during_cleanup() {
        let pid = AtomicU32::new(0);
        let packet =
            json!({"protocolVersion":1,"executionId":"fixed","type":"started","browserPid":12345});
        register_cleanup_packet(&packet, "fixed", 12344, &pid).unwrap();
        assert_eq!(pid.load(Ordering::Acquire), 12345);
        register_cleanup_packet(&packet, "fixed", 12344, &pid).unwrap();
        for altered in [
            json!({"protocolVersion":1,"executionId":"different","type":"started","browserPid":12345}),
            json!({"protocolVersion":1,"executionId":"fixed","type":"started","browserPid":12344}),
            json!({"protocolVersion":1,"executionId":"fixed","type":"started","browserPid":12346}),
            json!({"protocolVersion":1,"executionId":"fixed","type":"started","browserPid":2147483648u64}),
        ] {
            assert!(register_cleanup_packet(&altered, "fixed", 12344, &pid).is_err());
            assert_eq!(pid.load(Ordering::Acquire), 12345);
        }
    }
}
