use crate::{CancellationToken, MediaError, MediaResult, MediaTime, OutputAdmission};
use std::{
    io::{BufRead, BufReader, Read},
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug)]
pub struct PreviewStreamLimits {
    pub max_total_bytes: u64,
    pub max_buffered_bytes: usize,
    pub max_block_duration: Duration,
}
impl Default for PreviewStreamLimits {
    fn default() -> Self {
        Self {
            max_total_bytes: 512 * 1024 * 1024,
            max_buffered_bytes: 4 * 1024 * 1024,
            max_block_duration: Duration::from_secs(10),
        }
    }
}

#[derive(Clone, Debug)]
pub struct PreviewStreamStatus {
    pub produced_through: Option<MediaTime>,
    pub buffered_bytes: u64,
    pub done: bool,
    pub error_code: Option<String>,
}

#[derive(Debug)]
pub enum PreviewStreamRead {
    Chunk(Vec<u8>),
    Timeout,
    End,
}

pub struct ContinuousPreviewProducer {
    receiver: Receiver<Vec<u8>>,
    cancel: CancellationToken,
    pid: Arc<std::sync::atomic::AtomicU32>,
    worker: Mutex<Option<JoinHandle<MediaResult<()>>>>,
    status: Arc<Mutex<PreviewStreamStatus>>,
}

impl ContinuousPreviewProducer {
    pub fn recv_timeout(&self, timeout: Duration) -> MediaResult<PreviewStreamRead> {
        match self.receiver.recv_timeout(timeout) {
            Ok(bytes) => {
                let mut status = self.status.lock().expect("preview status mutex");
                status.buffered_bytes = status.buffered_bytes.saturating_sub(bytes.len() as u64);
                Ok(PreviewStreamRead::Chunk(bytes))
            }
            Err(RecvTimeoutError::Timeout) => Ok(PreviewStreamRead::Timeout),
            Err(RecvTimeoutError::Disconnected) => {
                if let Some(worker) = self.worker.lock().expect("preview worker mutex").take() {
                    worker.join().map_err(|_| MediaError::InvalidToolOutput {
                        tool: "ffmpeg-preview".into(),
                        detail: "producer thread panicked".into(),
                    })??;
                }
                Ok(PreviewStreamRead::End)
            }
        }
    }
    pub fn status(&self) -> PreviewStreamStatus {
        self.status.lock().expect("preview status mutex").clone()
    }
    pub fn cancel(&self) {
        self.cancel.cancel();
        #[cfg(unix)]
        {
            use std::sync::atomic::Ordering;
            let pid = self.pid.load(Ordering::Acquire);
            if pid != 0 {
                unsafe {
                    libc::kill(pid as i32, libc::SIGTERM);
                }
            }
        }
    }
}
impl Drop for ContinuousPreviewProducer {
    fn drop(&mut self) {
        self.cancel();
        if let Some(worker) = self.worker.lock().expect("preview worker mutex").take() {
            let _ = worker.join();
        }
    }
}

pub(crate) fn spawn(
    command: Command,
    cancel: CancellationToken,
    limits: PreviewStreamLimits,
    admission: Option<Arc<dyn OutputAdmission>>,
    cleanup: Vec<PathBuf>,
) -> MediaResult<ContinuousPreviewProducer> {
    if limits.max_total_bytes == 0
        || !(65_536..=64 * 1024 * 1024).contains(&limits.max_buffered_bytes)
        || limits.max_block_duration.is_zero()
        || limits.max_block_duration > Duration::from_secs(60)
    {
        return Err(MediaError::InvalidRequest(
            "preview stream limits outside supported bounds".into(),
        ));
    }
    let slots = (limits.max_buffered_bytes / 65_536).max(1);
    let (sender, receiver) = mpsc::sync_channel(slots);
    let status = Arc::new(Mutex::new(PreviewStreamStatus {
        produced_through: None,
        buffered_bytes: 0,
        done: false,
        error_code: None,
    }));
    let pid = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let worker_status = status.clone();
    let worker_pid = pid.clone();
    let worker_cancel = cancel.clone();
    let worker = thread::spawn(move || {
        struct Cleanup(Vec<PathBuf>);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                for path in &self.0 {
                    let _ = std::fs::remove_file(path);
                }
            }
        }
        let _cleanup = Cleanup(cleanup);
        #[cfg(unix)]
        let watcher_done = Arc::new(std::sync::atomic::AtomicBool::new(false));
        #[cfg(unix)]
        let watcher = {
            use std::sync::atomic::Ordering;
            let done = watcher_done.clone();
            let token = worker_cancel.clone();
            let process = worker_pid.clone();
            thread::spawn(move || {
                let mut terminated = None;
                while !done.load(Ordering::Acquire) {
                    if token.is_cancelled() {
                        let value = process.load(Ordering::Acquire);
                        if value != 0 {
                            let signal = if terminated
                                .is_some_and(|at: Instant| at.elapsed() >= Duration::from_secs(2))
                            {
                                libc::SIGKILL
                            } else {
                                libc::SIGTERM
                            };
                            if terminated.is_none() {
                                terminated = Some(Instant::now());
                            }
                            unsafe {
                                libc::kill(value as i32, signal);
                            }
                        }
                    }
                    thread::sleep(Duration::from_millis(20));
                }
            })
        };
        let result = run(
            command,
            sender,
            &worker_cancel,
            limits,
            admission.as_deref(),
            &worker_status,
            &worker_pid,
        );
        #[cfg(unix)]
        {
            use std::sync::atomic::Ordering;
            watcher_done.store(true, Ordering::Release);
            let _ = watcher.join();
        }
        let mut status = worker_status.lock().expect("preview status mutex");
        status.done = true;
        if let Err(error) = &result {
            status.error_code = Some(error.code().into());
        }
        result
    });
    Ok(ContinuousPreviewProducer {
        receiver,
        cancel,
        pid,
        worker: Mutex::new(Some(worker)),
        status,
    })
}

fn run(
    mut command: Command,
    sender: SyncSender<Vec<u8>>,
    cancel: &CancellationToken,
    limits: PreviewStreamLimits,
    admission: Option<&dyn OutputAdmission>,
    status: &Arc<Mutex<PreviewStreamStatus>>,
    pid: &Arc<std::sync::atomic::AtomicU32>,
) -> MediaResult<()> {
    if cancel.is_cancelled() {
        return Err(MediaError::Cancelled);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut owned = crate::process::spawn_command(&mut command, "ffmpeg-preview", admission, None)?;
    let child = owned.child();
    use std::sync::atomic::Ordering;
    pid.store(child.id(), Ordering::Release);
    let mut stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let progress_status = status.clone();
    let stderr_reader = thread::spawn(move || {
        let mut tail = String::new();
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if let Some(value) = line.strip_prefix("out_time_us=")
                && let Ok(ticks) = value.parse::<i64>()
                && ticks >= 0
            {
                progress_status
                    .lock()
                    .expect("preview status mutex")
                    .produced_through = Some(MediaTime {
                    ticks,
                    timebase: 1_000_000,
                });
            }
            if tail.len() < 4096 {
                tail.push_str(&line.chars().take(256).collect::<String>());
                tail.push('\n');
            }
        }
        tail
    });
    let result = (|| {
        let mut total = 0u64;
        let mut saw_ftyp = false;
        let mut saw_moov = false;
        let mut saw_moof = false;
        let mut saw_mdat = false;
        loop {
            if cancel.is_cancelled() {
                return Err(MediaError::Cancelled);
            }
            let Some(box_bytes) = read_box(&mut stdout, limits.max_buffered_bytes)? else {
                break;
            };
            let kind = &box_bytes[4..8];
            saw_ftyp |= kind == b"ftyp";
            saw_moov |= kind == b"moov";
            saw_moof |= kind == b"moof";
            saw_mdat |= kind == b"mdat";
            total = total
                .checked_add(box_bytes.len() as u64)
                .ok_or(MediaError::OutputLimit {
                    limit_bytes: limits.max_total_bytes,
                })?;
            if total > limits.max_total_bytes {
                return Err(MediaError::OutputLimit {
                    limit_bytes: limits.max_total_bytes,
                });
            }
            send_bounded(
                &sender,
                box_bytes,
                cancel,
                limits.max_block_duration,
                limits.max_buffered_bytes,
                status,
            )?;
        }
        if !(saw_ftyp && saw_moov && saw_moof && saw_mdat) {
            return Err(MediaError::InvalidToolOutput {
                tool: "ffmpeg-preview".into(),
                detail: "fragmented MP4 init/media boxes missing".into(),
            });
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = child.kill();
    }
    let exit = child.wait()?;
    pid.store(0, Ordering::Release);
    let stderr = stderr_reader.join().unwrap_or_default();
    result?;
    if !exit.success() {
        return Err(MediaError::ProcessFailed {
            tool: "ffmpeg-preview".into(),
            status: exit.code(),
            stderr,
        });
    }
    Ok(())
}

fn read_box(reader: &mut impl Read, max_box_bytes: usize) -> MediaResult<Option<Vec<u8>>> {
    let mut header = [0u8; 8];
    let mut count = 0;
    while count < 8 {
        let got = reader.read(&mut header[count..])?;
        if got == 0 {
            if count == 0 {
                return Ok(None);
            }
            return Err(MediaError::InvalidToolOutput {
                tool: "ffmpeg-preview".into(),
                detail: "truncated MP4 box header".into(),
            });
        }
        count += got;
    }
    let size = u32::from_be_bytes(header[..4].try_into().expect("four bytes")) as usize;
    if size < 8 || size > max_box_bytes {
        return Err(MediaError::OutputLimit {
            limit_bytes: max_box_bytes as u64,
        });
    }
    let mut bytes = vec![0u8; size];
    bytes[..8].copy_from_slice(&header);
    reader.read_exact(&mut bytes[8..])?;
    Ok(Some(bytes))
}

fn send_bounded(
    sender: &SyncSender<Vec<u8>>,
    bytes: Vec<u8>,
    cancel: &CancellationToken,
    max_block: Duration,
    max_buffered_bytes: usize,
    status: &Arc<Mutex<PreviewStreamStatus>>,
) -> MediaResult<()> {
    let size = bytes.len() as u64;
    let mut pending = bytes;
    let started = Instant::now();
    loop {
        if cancel.is_cancelled() {
            return Err(MediaError::Cancelled);
        }
        let mut state = status.lock().expect("preview status mutex");
        if state.buffered_bytes + size <= max_buffered_bytes as u64 {
            match sender.try_send(pending) {
                Ok(()) => {
                    state.buffered_bytes += size;
                    return Ok(());
                }
                Err(TrySendError::Disconnected(_)) => return Err(MediaError::Cancelled),
                Err(TrySendError::Full(bytes)) => pending = bytes,
            }
        }
        drop(state);
        if started.elapsed() >= max_block {
            return Err(MediaError::Backpressure(
                "consumer did not drain the bounded stream in time".into(),
            ));
        }
        thread::sleep(Duration::from_millis(10));
    }
}
