use crate::{MediaError, MediaResult, MediaTime, ProgressStage, ProgressUpdate};
use std::{
    io::{BufRead, BufReader, Read},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

#[derive(Clone, Default, Debug)]
pub struct CancellationToken(Arc<AtomicBool>);
impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Takes ownership of a one-shot builder. All managed children, including
/// readers, retain the attempt lifetime; only payload writers reserve bytes.
pub(crate) fn spawn_command(
    command: &mut Command,
    tool: &str,
    admission: Option<&dyn crate::OutputAdmission>,
    output: Option<&std::path::Path>,
) -> MediaResult<Box<dyn crate::OutputProcess>> {
    let mut command = std::mem::replace(command, Command::new(""));
    if let Some(admission) = admission {
        return admission.spawn(command, output);
    }
    Ok(Box::new(crate::admission::UnmanagedProcess(
        command.spawn().map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => MediaError::ToolUnavailable {
                tool: tool.into(),
                detail: error.to_string(),
            },
            _ => MediaError::Io(error),
        })?,
    )))
}

pub(crate) fn checked_output(
    command: &mut Command,
    tool: &str,
    admission: Option<&dyn crate::OutputAdmission>,
) -> MediaResult<std::process::Output> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut owned = spawn_command(command, tool, admission, None)?;
    let child = owned.child();
    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut stderr = child.stderr.take().expect("piped stderr");
    let out = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).map(|_| bytes)
    });
    let err = thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).map(|_| bytes)
    });
    let status = child.wait();
    if status.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    let stdout = out
        .join()
        .map_err(|_| std::io::Error::other("Probe reader failed"))??;
    let stderr = err
        .join()
        .map_err(|_| std::io::Error::other("Probe reader failed"))??;
    let status = status?;
    if status.success() {
        Ok(std::process::Output {
            status,
            stdout,
            stderr,
        })
    } else {
        Err(MediaError::ProcessFailed {
            tool: tool.into(),
            status: status.code(),
            stderr: String::from_utf8_lossy(&stderr).trim().into(),
        })
    }
}

pub(crate) fn run_ffmpeg(
    mut command: Command,
    cancel: &CancellationToken,
    duration: Option<f64>,
    stage: ProgressStage,
    progress: &mut dyn FnMut(ProgressUpdate),
    admission: Option<(Option<&std::path::Path>, &dyn crate::OutputAdmission)>,
) -> MediaResult<()> {
    progress(ProgressUpdate {
        stage,
        fraction: None,
        media_time: None,
    });
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut owned = spawn_command(
        &mut command,
        "ffmpeg",
        admission.map(|(_, a)| a),
        admission.and_then(|(path, _)| path),
    )?;
    let ceiling = owned.output_limit();
    let child = owned.child();
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let stderr_reader = thread::spawn(move || {
        let mut value = String::new();
        let _ = BufReader::new(stderr).read_to_string(&mut value);
        value
    });
    let (tx, rx) = std::sync::mpsc::channel();
    let stdout_reader = thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let _ = tx.send(line);
        }
    });
    let status = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        monitor(child, cancel, duration, stage, progress, &rx)
    }));
    if let Err(payload) = status {
        let _ = child.kill();
        let _ = child.wait();
        let _ = stdout_reader.join();
        let _ = stderr_reader.join();
        std::panic::resume_unwind(payload);
    }
    let status = status.expect("panic branch resumed unwinding");
    if status.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    let _ = stdout_reader.join();
    while let Ok(line) = rx.try_recv() {
        emit_progress_line(&line, duration, stage, progress);
    }
    let stderr = stderr_reader
        .join()
        .unwrap_or_else(|_| "failed to collect ffmpeg stderr".into());
    let status = status?;
    if let Some(limit_bytes) = ceiling {
        #[cfg(unix)]
        let size_signal = {
            use std::os::unix::process::ExitStatusExt;
            status.signal() == Some(libc::SIGXFSZ)
        };
        #[cfg(not(unix))]
        let size_signal = false;
        let reached = admission
            .and_then(|(path, _)| path.and_then(|path| std::fs::metadata(path).ok()))
            .is_some_and(|m| m.len() >= limit_bytes);
        if size_signal || reached {
            return Err(MediaError::OutputLimit { limit_bytes });
        }
    }
    if status.success() {
        Ok(())
    } else {
        Err(MediaError::ProcessFailed {
            tool: "ffmpeg".into(),
            status: status.code(),
            stderr: stderr.trim().into(),
        })
    }
}

fn monitor(
    child: &mut Child,
    cancel: &CancellationToken,
    duration: Option<f64>,
    stage: ProgressStage,
    progress: &mut dyn FnMut(ProgressUpdate),
    rx: &std::sync::mpsc::Receiver<String>,
) -> MediaResult<std::process::ExitStatus> {
    loop {
        if cancel.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(MediaError::Cancelled);
        }
        while let Ok(line) = rx.try_recv() {
            emit_progress_line(&line, duration, stage, progress);
        }
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn emit_progress_line(
    line: &str,
    duration: Option<f64>,
    stage: ProgressStage,
    progress: &mut dyn FnMut(ProgressUpdate),
) {
    if let Some(raw) = line.strip_prefix("out_time_us=")
        && let Ok(us) = raw.parse::<i64>()
    {
        let seconds = us as f64 / 1_000_000.0;
        progress(ProgressUpdate {
            stage,
            fraction: duration.map(|d| (seconds / d).clamp(0.0, 1.0) as f32),
            media_time: Some(MediaTime {
                ticks: us,
                timebase: 1_000_000,
            }),
        });
    }
}

/// Bounded, cancellable capture for probing uploaded bytes. Both pipes are drained
/// concurrently; exceeding a limit terminates and reaps the child before returning.
pub(crate) fn checked_output_limited(
    command: &mut Command,
    tool: &str,
    cancel: &CancellationToken,
    timeout: Duration,
    admission: Option<&dyn crate::OutputAdmission>,
) -> MediaResult<Vec<u8>> {
    if cancel.is_cancelled() {
        return Err(MediaError::Cancelled);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut owned = spawn_command(command, tool, admission, None)?;
    let child = owned.child();
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let (sender, receiver) = std::sync::mpsc::channel();
    let out = sender.clone();
    let stdout_reader = thread::spawn(move || {
        let mut value = Vec::new();
        let result = stdout.take(1_048_577).read_to_end(&mut value);
        let _ = out.send((true, result.map(|_| value)));
    });
    let stderr_reader = thread::spawn(move || {
        let mut value = Vec::new();
        let result = stderr.take(65_537).read_to_end(&mut value);
        let _ = sender.send((false, result.map(|_| value)));
    });
    let started = std::time::Instant::now();
    let mut stdout = None;
    let mut stderr = None;
    let outcome = loop {
        if cancel.is_cancelled() {
            break Err(MediaError::Cancelled);
        }
        if started.elapsed() >= timeout {
            break Err(MediaError::InvalidToolOutput {
                tool: tool.into(),
                detail: "Probe time budget exceeded".into(),
            });
        }
        match receiver.recv_timeout(Duration::from_millis(10)) {
            Ok((is_stdout, result)) => match result {
                Ok(bytes) => {
                    if bytes.len() > if is_stdout { 1_048_576 } else { 65_536 } {
                        break Err(MediaError::InvalidToolOutput {
                            tool: tool.into(),
                            detail: "Probe output budget exceeded".into(),
                        });
                    }
                    if is_stdout {
                        stdout = Some(bytes);
                    } else {
                        stderr = Some(bytes);
                    }
                }
                Err(e) => break Err(MediaError::Io(e)),
            },
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected)
                if stdout.is_none() || stderr.is_none() =>
            {
                break Err(MediaError::InvalidToolOutput {
                    tool: tool.into(),
                    detail: "Probe output reader stopped".into(),
                });
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                thread::sleep(Duration::from_millis(10))
            }
            _ => {}
        }
        match child.try_wait() {
            Ok(Some(status)) if stdout.is_some() && stderr.is_some() => {
                if status.success() {
                    break Ok(stdout.take().unwrap());
                }
                break Err(MediaError::ProcessFailed {
                    tool: tool.into(),
                    status: status.code(),
                    stderr: String::from_utf8_lossy(stderr.as_ref().unwrap())
                        .trim()
                        .into(),
                });
            }
            Err(e) => break Err(MediaError::Io(e)),
            _ => {}
        }
    };
    if outcome.is_err() {
        let _ = child.kill();
    }
    let _ = child.wait();
    let _ = stdout_reader.join();
    let _ = stderr_reader.join();
    outcome
}

#[cfg(test)]
mod bounded_capture_tests {
    use super::*;
    #[test]
    fn uploaded_probe_capture_bounds_output_and_reaps_after_cancel_or_timeout() {
        for script in [
            "import sys; sys.stdout.write('x'*2000000)",
            "import sys; sys.stderr.write('x'*200000)",
        ] {
            let error = checked_output_limited(
                Command::new("python3").args(["-c", script]),
                "fixture",
                &CancellationToken::new(),
                Duration::from_secs(5),
                None,
            )
            .unwrap_err();
            assert!(matches!(error, MediaError::InvalidToolOutput { .. }));
        }
        let cancel = CancellationToken::new();
        let signal = cancel.clone();
        let thread = thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            signal.cancel();
        });
        let start = std::time::Instant::now();
        let error = checked_output_limited(
            Command::new("python3").args(["-c", "import time; time.sleep(30)"]),
            "fixture",
            &cancel,
            Duration::from_secs(5),
            None,
        )
        .unwrap_err();
        thread.join().unwrap();
        assert!(matches!(error, MediaError::Cancelled));
        assert!(start.elapsed() < Duration::from_secs(3));
        let error = checked_output_limited(
            Command::new("python3").args(["-c", "import time; time.sleep(30)"]),
            "fixture",
            &CancellationToken::new(),
            Duration::from_millis(100),
            None,
        )
        .unwrap_err();
        assert!(matches!(error, MediaError::InvalidToolOutput { .. }));
        let bytes = checked_output_limited(
            Command::new("python3").args(["-c", "print('ok')"]),
            "fixture",
            &CancellationToken::new(),
            Duration::from_secs(5),
            None,
        )
        .unwrap();
        assert_eq!(bytes, b"ok\n");
    }
}
