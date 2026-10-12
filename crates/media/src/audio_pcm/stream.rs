use super::*;
use crate::OutputAdmission;
use std::{
    io::Read,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::Instant,
};

const FRAMES: usize = 4096;
const BYTES: usize = FRAMES * 2 * 4;

/// Owns the admitted child and its attempt lease until it is reaped. Only a
/// fixed number of blocks can queue. Dropping one voice cancels its child, never
/// the caller's job token or another voice.
pub struct NativePcmStream {
    receiver: Receiver<NativePcmBlock>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<MediaResult<u64>>>,
    finished_samples: Option<u64>,
    timeout: Duration,
}
impl NativePcmStream {
    pub fn read(&mut self, cancel: &CancellationToken) -> MediaResult<Option<NativePcmBlock>> {
        if self.finished_samples.is_some() {
            return Ok(None);
        }
        let started = Instant::now();
        loop {
            if cancel.is_cancelled() {
                self.stop.store(true, Ordering::Release);
                return Err(MediaError::Cancelled);
            }
            match self.receiver.recv_timeout(Duration::from_millis(20)) {
                Ok(block) => return Ok(Some(block)),
                Err(RecvTimeoutError::Disconnected) => {
                    let worker = self
                        .worker
                        .take()
                        .ok_or_else(|| invalid("PCM worker missing"))?;
                    let frames = worker
                        .join()
                        .map_err(|_| invalid("PCM worker panicked"))??;
                    self.finished_samples = Some(frames);
                    return Ok(None);
                }
                Err(RecvTimeoutError::Timeout) => {
                    if started.elapsed() >= self.timeout {
                        self.stop.store(true, Ordering::Release);
                        return Err(MediaError::Backpressure("PCM decoder stalled".into()));
                    }
                }
            }
        }
    }
    pub fn decoded_frame_count(&self) -> Option<u64> {
        self.finished_samples
    }
}
impl Drop for NativePcmStream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub(super) fn spawn(
    mut command: Command,
    limits: PcmDecodeLimits,
    admission: Option<Arc<dyn OutputAdmission>>,
    cancel: CancellationToken,
) -> MediaResult<NativePcmStream> {
    limits.validate()?;
    {
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let (sender, receiver) = mpsc::sync_channel(limits.max_buffered_blocks);
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let worker =
            thread::spawn(move || run(command, sender, limits, worker_stop, admission, cancel));
        Ok(NativePcmStream {
            receiver,
            stop,
            worker: Some(worker),
            finished_samples: None,
            timeout: limits.stall_timeout,
        })
    }
}

fn run(
    mut command: Command,
    sender: SyncSender<NativePcmBlock>,
    limits: PcmDecodeLimits,
    stop: Arc<AtomicBool>,
    admission: Option<Arc<dyn OutputAdmission>>,
    cancel: CancellationToken,
) -> MediaResult<u64> {
    if stop.load(Ordering::Acquire) {
        return Err(MediaError::Cancelled);
    }
    // The lease-bearing OutputProcess need not be Send: construct and retain it
    // entirely within this worker, exactly like the existing preview producer.
    let mut owned =
        crate::process::spawn_command(&mut command, "ffmpeg-pcm", admission.as_deref(), None)?;
    let child = owned.child();
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| invalid("Missing PCM pipe"))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| invalid("Missing PCM stderr"))?;
    struct StopOnDrop(Arc<AtomicBool>);
    impl Drop for StopOnDrop {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Release);
        }
    }
    let _stop_on_drop = StopOnDrop(stop.clone());
    let stderr_worker = thread::spawn(move || {
        let mut tail = Vec::new();
        let mut buffer = [0u8; 4096];
        while let Ok(count) = stderr.read(&mut buffer) {
            if count == 0 {
                break;
            }
            let keep = count.min(65536usize.saturating_sub(tail.len()));
            tail.extend_from_slice(&buffer[..keep]);
        }
        String::from_utf8_lossy(&tail).into_owned()
    });
    let (result_sender, result_receiver) = mpsc::sync_channel(1);
    let reader_stop = stop.clone();
    let stdout_worker = thread::spawn(move || {
        let result = (|| {
            let mut frames = 0u64;
            loop {
                if reader_stop.load(Ordering::Acquire) {
                    return Err(MediaError::Cancelled);
                }
                let mut bytes = vec![0u8; BYTES];
                let mut count = 0;
                while count < BYTES {
                    let got = stdout.read(&mut bytes[count..])?;
                    if got == 0 {
                        break;
                    }
                    count += got;
                }
                if count == 0 {
                    break;
                }
                if count % 8 != 0 {
                    return Err(invalid("Partial interleaved PCM frame"));
                }
                let next =
                    frames
                        .checked_add((count / 8) as u64)
                        .ok_or(MediaError::OutputLimit {
                            limit_bytes: limits.max_decoded_bytes,
                        })?;
                if next
                    .checked_mul(8)
                    .is_none_or(|n| n > limits.max_decoded_bytes)
                {
                    return Err(MediaError::OutputLimit {
                        limit_bytes: limits.max_decoded_bytes,
                    });
                }
                let samples: Vec<f32> = bytes[..count]
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes(b.try_into().expect("four bytes")))
                    .collect();
                if samples.iter().any(|v| !v.is_finite()) {
                    return Err(invalid("Decoded PCM contains non-finite values"));
                }
                send(
                    &sender,
                    NativePcmBlock {
                        start_sample: frames,
                        samples,
                    },
                    &reader_stop,
                    limits.stall_timeout,
                )?;
                frames = next;
            }
            Ok(frames)
        })();
        let _ = result_sender.send(result);
    });
    // The thread owning the admitted Child performs cancellation directly;
    // no detached PID watcher can signal a reused process after reaping.
    let mut outcome = None;
    let mut status = None;
    let mut killed = false;
    let mut completion_started = None;
    loop {
        if (stop.load(Ordering::Acquire) || cancel.is_cancelled())
            && outcome.as_ref().is_none_or(Result::is_ok)
        {
            outcome = Some(Err(MediaError::Cancelled));
        }
        if outcome.as_ref().is_some_and(Result::is_ok) && status.is_none() {
            let at = completion_started.get_or_insert_with(Instant::now);
            if at.elapsed() >= limits.stall_timeout {
                outcome = Some(Err(MediaError::Backpressure(
                    "PCM decoder did not exit after EOF".into(),
                )));
            }
        }
        if outcome.as_ref().is_some_and(Result::is_err) && status.is_none() && !killed {
            stop.store(true, Ordering::Release);
            let _ = child.kill();
            killed = true;
        }
        if status.is_none() {
            match child.try_wait() {
                Ok(Some(exit)) => status = Some(Ok(exit)),
                Ok(None) => {}
                Err(e) => {
                    stop.store(true, Ordering::Release);
                    let _ = child.kill();
                    let _ = child.wait();
                    status = Some(Err(e));
                }
            }
        }
        if outcome.is_none() {
            match result_receiver.recv_timeout(Duration::from_millis(10)) {
                Ok(result) => outcome = Some(result),
                Err(RecvTimeoutError::Disconnected) => {
                    outcome = Some(Err(invalid("PCM reader stopped")))
                }
                Err(RecvTimeoutError::Timeout) => {}
            }
        } else if status.is_none() {
            thread::sleep(Duration::from_millis(10));
        }
        if outcome.is_some() && status.is_some() {
            break;
        }
    }
    let _ = stdout_worker.join();
    let stderr = stderr_worker
        .join()
        .unwrap_or_else(|_| "PCM stderr worker panicked".into());
    let frames = outcome.expect("reader outcome checked")?;
    let status = status.expect("process exit checked")?;
    if !status.success() {
        return Err(MediaError::ProcessFailed {
            tool: "ffmpeg-pcm".into(),
            status: status.code(),
            stderr,
        });
    }
    Ok(frames)
}
fn send(
    sender: &SyncSender<NativePcmBlock>,
    mut block: NativePcmBlock,
    stop: &AtomicBool,
    timeout: Duration,
) -> MediaResult<()> {
    let start = Instant::now();
    loop {
        if stop.load(Ordering::Acquire) {
            return Err(MediaError::Cancelled);
        }
        match sender.try_send(block) {
            Ok(()) => return Ok(()),
            Err(TrySendError::Disconnected(_)) => return Err(MediaError::Cancelled),
            Err(TrySendError::Full(pending)) => block = pending,
        }
        if start.elapsed() >= timeout {
            return Err(MediaError::Backpressure(
                "PCM consumer did not drain its bounded queue".into(),
            ));
        }
        thread::sleep(Duration::from_millis(10));
    }
}
