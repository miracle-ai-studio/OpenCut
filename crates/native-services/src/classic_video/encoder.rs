use opencut_contracts::EditorError;
use opencut_editor_application::classic_visual::ClassicVisualRgbaSink;
use opencut_media::{OutputAdmission, OutputProcess};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{ChildStdin, Command, Stdio},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
fn err(message: &str) -> EditorError {
    EditorError::new("VIDEO_ENCODER_FAILED", message)
}
fn io(error: std::io::Error) -> EditorError {
    err(&error.to_string())
}
fn media(error: opencut_media::MediaError) -> EditorError {
    match error {
        opencut_media::MediaError::Admission { code, message } => EditorError::new(code, message),
        other => err(&other.to_string()),
    }
}
/// Resolve once, execute and hash the exact file, and rehash after the attempt.
/// PATH resolution never delegates to a shell or executes an internal CLI.
pub fn resolve_tool(value: PathBuf) -> Result<PathBuf, EditorError> {
    if value.is_absolute() || value.components().count() != 1 {
        return checked_executable(&value);
    }
    let paths = std::env::var_os("PATH").ok_or_else(tool_unavailable)?;
    resolve_in_paths(&value, &paths)
}
fn resolve_in_paths(value: &Path, paths: &std::ffi::OsStr) -> Result<PathBuf, EditorError> {
    for directory in std::env::split_paths(paths) {
        if let Ok(executable) = checked_executable(&directory.join(value)) {
            return Ok(executable);
        }
    }
    Err(tool_unavailable())
}
fn tool_unavailable() -> EditorError {
    EditorError::new(
        "CAPABILITY_UNAVAILABLE",
        "Required video encoder/probe is not an executable regular file",
    )
}
fn checked_executable(path: &Path) -> Result<PathBuf, EditorError> {
    let resolved = std::fs::canonicalize(path).map_err(|_| tool_unavailable())?;
    let metadata = std::fs::metadata(&resolved).map_err(|_| tool_unavailable())?;
    if !metadata.is_file() {
        return Err(tool_unavailable());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err(tool_unavailable());
        }
    }
    Ok(resolved)
}

fn hash(
    path: &Path,
    checkpoint: &mut dyn FnMut() -> Result<(), EditorError>,
) -> Result<String, EditorError> {
    let mut file = File::open(path).map_err(io)?;
    let mut digest = Sha256::new();
    let mut bytes = [0u8; 65536];
    loop {
        checkpoint()?;
        let n = file.read(&mut bytes).map_err(io)?;
        if n == 0 {
            break;
        }
        digest.update(&bytes[..n]);
    }
    Ok(format!("{:x}", digest.finalize()))
}
pub struct ClassicVideoEncoderConfig {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub output: PathBuf,
    pub audio: PathBuf,
    pub width: u32,
    pub height: u32,
    pub fps_numerator: u32,
    pub fps_denominator: u32,
    pub frame_count: u64,
    pub max_output_bytes: u64,
    pub max_rss_bytes: u64,
    pub deadline: Duration,
}
pub struct ClassicVideoEncoderObservation {
    pub ffmpeg_sha256: String,
    pub ffprobe_sha256: String,
    pub video_frames: u64,
    pub video_duration_seconds: f64,
    pub audio_duration_seconds: f64,
}
pub struct ClassicVideoEncoder<F: FnMut() -> Result<(), EditorError>> {
    process: Option<Box<dyn OutputProcess>>,
    input: Option<ChildStdin>,
    stderr: Option<JoinHandle<std::io::Result<Vec<u8>>>>,
    config: ClassicVideoEncoderConfig,
    checkpoint: F,
    started: Instant,
    last_rss: Instant,
    ffmpeg_hash: String,
    ffprobe_hash: String,
    next_frame: u64,
    offset: u64,
    export_options: Option<opencut_contracts::export_video_v2::ExportVideoOptionsV2>,
}
impl<F: FnMut() -> Result<(), EditorError>> ClassicVideoEncoder<F> {
    #[cfg(unix)]
    pub fn start(
        config: ClassicVideoEncoderConfig,
        admission: &dyn OutputAdmission,
        checkpoint: F,
    ) -> Result<Self, EditorError> {
        Self::start_internal(config, admission, checkpoint, None)
    }
    #[cfg(unix)]
    pub fn start_v2(
        config: ClassicVideoEncoderConfig,
        options: opencut_contracts::export_video_v2::ExportVideoOptionsV2,
        admission: &dyn OutputAdmission,
        checkpoint: F,
    ) -> Result<Self, EditorError> {
        Self::start_internal(config, admission, checkpoint, Some(options))
    }
    #[cfg(unix)]
    fn start_internal(
        config: ClassicVideoEncoderConfig,
        admission: &dyn OutputAdmission,
        mut checkpoint: F,
        export_options: Option<opencut_contracts::export_video_v2::ExportVideoOptionsV2>,
    ) -> Result<Self, EditorError> {
        if config.width == 0
            || config.height == 0
            || config.width % 2 != 0
            || config.height % 2 != 0
            || config.fps_numerator == 0
            || config.fps_denominator == 0
            || config.frame_count == 0
            || config.frame_count > 1800
            || config.max_output_bytes == 0
            || config.deadline.is_zero()
        {
            return Err(err("Invalid basic CFR encoder admission"));
        }
        let ffmpeg_hash = hash(&config.ffmpeg, &mut checkpoint)?;
        let ffprobe_hash = hash(&config.ffprobe, &mut checkpoint)?;
        let mut command = Command::new(&config.ffmpeg);
        if let Some(options) = &export_options {
            crate::export_video_v2::append_arguments(&mut command, &config, options)?;
        } else {
            command
                .args([
                    "-hide_banner",
                    "-loglevel",
                    "error",
                    "-nostdin",
                    "-n",
                    "-protocol_whitelist",
                    "file,pipe",
                    "-f",
                    "rawvideo",
                    "-pixel_format",
                    "rgba",
                    "-video_size",
                ])
                .arg(format!("{}x{}", config.width, config.height))
                .arg("-framerate")
                .arg(format!(
                    "{}/{}",
                    config.fps_numerator, config.fps_denominator
                ))
                .args(["-i", "pipe:0", "-i"])
                .arg(&config.audio)
                .args([
                    "-map",
                    "0:v:0",
                    "-map",
                    "1:a:0",
                    "-c:v",
                    "libx264",
                    "-pix_fmt",
                    "yuv420p",
                    "-crf",
                    "18",
                    "-preset",
                    "medium",
                    "-threads",
                    "2",
                    "-filter_threads",
                    "1",
                    "-filter_complex_threads",
                    "1",
                    "-c:a",
                    "aac",
                    "-b:a",
                    "192000",
                    "-ar",
                    "48000",
                    "-ac",
                    "2",
                    "-movflags",
                    "+faststart",
                    "-f",
                    "mp4",
                ])
                .arg(&config.output);
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let started = Instant::now();
        let mut process = admission
            .spawn(command, Some(&config.output))
            .map_err(media)?;
        let input = process
            .child()
            .stdin
            .take()
            .ok_or_else(|| err("Missing encoder input"))?;
        nonblocking(&input)?;
        let stderr = process
            .child()
            .stderr
            .take()
            .ok_or_else(|| err("Missing encoder diagnostics"))?;
        let stderr = thread::spawn(move || bounded_read(stderr, 16384));
        Ok(Self {
            process: Some(process),
            input: Some(input),
            stderr: Some(stderr),
            config,
            checkpoint,
            started,
            last_rss: started,
            ffmpeg_hash,
            ffprobe_hash,
            next_frame: 0,
            offset: 0,
            export_options,
        })
    }
    fn check(&mut self) -> Result<(), EditorError> {
        (self.checkpoint)()?;
        if self.started.elapsed() >= self.config.deadline {
            return Err(EditorError::new(
                "VIDEO_TIME_LIMIT",
                "Video encoding exceeded captured deadline",
            ));
        }
        if let Ok(metadata) = std::fs::metadata(&self.config.output) {
            if metadata.len() > self.config.max_output_bytes {
                return Err(EditorError::new(
                    "VIDEO_BUDGET_EXCEEDED",
                    "MP4 exceeded captured output admission",
                ));
            }
        }
        if self.last_rss.elapsed() >= Duration::from_millis(100) {
            self.last_rss = Instant::now();
            if let Some(process) = self.process.as_mut() {
                if let Some(rss) = resident_bytes(process.child().id())? {
                    if rss > self.config.max_rss_bytes {
                        return Err(EditorError::new(
                            "VIDEO_BUDGET_EXCEEDED",
                            "Encoder RSS exceeded captured admission",
                        ));
                    }
                }
            }
        }
        Ok(())
    }
    pub fn finish_frame(&mut self) -> Result<(), EditorError> {
        self.check()?;
        if self.next_frame >= self.config.frame_count
            || self.offset != u64::from(self.config.width) * u64::from(self.config.height) * 4
        {
            return Err(err("Encoder frame lacks complete validated RGBA"));
        }
        self.next_frame += 1;
        self.offset = 0;
        Ok(())
    }
    #[cfg(unix)]
    pub fn finish(
        mut self,
        admission: &dyn OutputAdmission,
    ) -> Result<ClassicVideoEncoderObservation, EditorError> {
        if self.next_frame != self.config.frame_count || self.offset != 0 {
            return Err(err("Encoder lacks all complete CFR frames"));
        }
        drop(self.input.take());
        let status = loop {
            self.check()?;
            if let Some(status) = self
                .process
                .as_mut()
                .unwrap()
                .child()
                .try_wait()
                .map_err(io)?
            {
                break status;
            }
            thread::sleep(Duration::from_millis(20));
        };
        let _diagnostics = self
            .stderr
            .take()
            .unwrap()
            .join()
            .map_err(|_| err("Encoder diagnostic reader failed"))?
            .map_err(io)?;
        let ceiling = self.process.as_ref().unwrap().output_limit();
        if !status.success() {
            use std::os::unix::process::ExitStatusExt;
            if status.signal() == Some(libc::SIGXFSZ)
                || ceiling.is_some_and(|limit| {
                    std::fs::metadata(&self.config.output).is_ok_and(|m| m.len() >= limit)
                })
            {
                return Err(EditorError::new(
                    "VIDEO_BUDGET_EXCEEDED",
                    "Encoder exhausted admitted output reservation",
                ));
            }
            return Err(err("FFmpeg encoding failed"));
        }
        drop(self.process.take());
        self.check()?;
        let remaining = self.config.deadline.saturating_sub(self.started.elapsed());
        let probe = probe(
            &self.config.ffprobe,
            &self.config.output,
            admission,
            remaining,
            &mut self.checkpoint,
        )?;
        self.check()?;
        if hash(&self.config.ffmpeg, &mut self.checkpoint)? != self.ffmpeg_hash
            || hash(&self.config.ffprobe, &mut self.checkpoint)? != self.ffprobe_hash
        {
            return Err(EditorError::new(
                "EXECUTION_IDENTITY_CHANGED",
                "Encoder/probe binary changed during execution",
            ));
        }
        if let Some(options) = &self.export_options {
            let observed = crate::export_video_v2::verify_probe(&probe, &self.config, options)?;
            return Ok(ClassicVideoEncoderObservation {
                ffmpeg_sha256: self.ffmpeg_hash.clone(),
                ffprobe_sha256: self.ffprobe_hash.clone(),
                video_frames: observed.frames,
                video_duration_seconds: observed.video_seconds,
                audio_duration_seconds: observed.audio_seconds.unwrap_or(0.0),
            });
        }
        let streams = probe
            .get("streams")
            .and_then(Value::as_array)
            .ok_or_else(|| err("Missing observed MP4 streams"))?;
        if streams.len() != 2 {
            return Err(err("MP4 must have exactly shared video and audio streams"));
        }
        let video = streams
            .iter()
            .find(|s| s.get("codec_type").and_then(Value::as_str) == Some("video"))
            .ok_or_else(|| err("Missing MP4 video"))?;
        let audio = streams
            .iter()
            .find(|s| s.get("codec_type").and_then(Value::as_str) == Some("audio"))
            .ok_or_else(|| err("Missing MP4 audio"))?;
        let count = video
            .get("nb_read_frames")
            .and_then(Value::as_str)
            .and_then(|v| v.parse::<u64>().ok())
            .ok_or_else(|| err("Missing observed decoded frame count"))?;
        let duration = |s: &Value| {
            s.get("duration")
                .and_then(Value::as_str)
                .and_then(|v| v.parse::<f64>().ok())
                .filter(|n| n.is_finite() && *n > 0.0)
                .ok_or_else(|| err("Missing observed stream duration"))
        };
        if count != self.config.frame_count
            || video.get("codec_name").and_then(Value::as_str) != Some("h264")
            || video.get("pix_fmt").and_then(Value::as_str) != Some("yuv420p")
            || video.get("width").and_then(Value::as_u64) != Some(u64::from(self.config.width))
            || video.get("height").and_then(Value::as_u64) != Some(u64::from(self.config.height))
            || audio.get("codec_name").and_then(Value::as_str) != Some("aac")
            || audio.get("sample_rate").and_then(Value::as_str) != Some("48000")
            || audio.get("channels").and_then(Value::as_u64) != Some(2)
        {
            return Err(err(
                "Observed MP4 stream format differs from fixed encoder profile",
            ));
        }
        Ok(ClassicVideoEncoderObservation {
            ffmpeg_sha256: self.ffmpeg_hash.clone(),
            ffprobe_sha256: self.ffprobe_hash.clone(),
            video_frames: count,
            video_duration_seconds: duration(video)?,
            audio_duration_seconds: duration(audio)?,
        })
    }
}
impl<F: FnMut() -> Result<(), EditorError>> ClassicVisualRgbaSink for ClassicVideoEncoder<F> {
    fn chunk(&mut self, offset: u64, bytes: &[u8]) -> Result<(), EditorError> {
        self.check()?;
        let frame_bytes = u64::from(self.config.width) * u64::from(self.config.height) * 4;
        if self.next_frame >= self.config.frame_count
            || offset != self.offset
            || bytes.is_empty()
            || bytes.len() > 65536
            || !bytes.len().is_multiple_of(4)
            || offset
                .checked_add(bytes.len() as u64)
                .is_none_or(|end| end > frame_bytes)
        {
            return Err(err("RGBA encoder stream changed frame order"));
        }
        let mut remaining = bytes;
        while !remaining.is_empty() {
            self.check()?;
            match self
                .input
                .as_mut()
                .ok_or_else(|| err("Encoder input closed"))?
                .write(remaining)
            {
                Ok(0) => return Err(err("Encoder input closed")),
                Ok(n) => remaining = &remaining[n..],
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10))
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => return Err(io(e)),
            }
        }
        self.offset += bytes.len() as u64;
        Ok(())
    }
}
impl<F: FnMut() -> Result<(), EditorError>> Drop for ClassicVideoEncoder<F> {
    fn drop(&mut self) {
        drop(self.input.take());
        drop(self.process.take());
        if let Some(reader) = self.stderr.take() {
            let _ = reader.join();
        }
    }
}
fn bounded_read(mut reader: impl Read, max: usize) -> std::io::Result<Vec<u8>> {
    let mut retained = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let n = reader.read(&mut chunk)?;
        if n == 0 {
            return Ok(retained);
        }
        let take = n.min(max.saturating_sub(retained.len()));
        retained.extend_from_slice(&chunk[..take]);
    }
}
#[cfg(unix)]
fn nonblocking(input: &ChildStdin) -> Result<(), EditorError> {
    use std::os::fd::AsRawFd;
    let fd = input.as_raw_fd();
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(io(std::io::Error::last_os_error()));
    }
    Ok(())
}
#[cfg(unix)]
fn probe(
    path: &Path,
    output: &Path,
    admission: &dyn OutputAdmission,
    deadline: Duration,
    checkpoint: &mut dyn FnMut() -> Result<(), EditorError>,
) -> Result<Value, EditorError> {
    checkpoint()?;
    let mut command = Command::new(path);
    command
        .args([
            "-v",
            "error",
            "-protocol_whitelist",
            "file,pipe",
            "-count_frames",
            "-show_streams",
            "-show_format",
            "-of",
            "json",
        ])
        .arg(output)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut process = admission.spawn(command, None).map_err(media)?;
    let stdout = process
        .child()
        .stdout
        .take()
        .ok_or_else(|| err("Missing probe output"))?;
    let stderr = process
        .child()
        .stderr
        .take()
        .ok_or_else(|| err("Missing probe diagnostics"))?;
    let out = thread::spawn(move || bounded_read(stdout, 1048577));
    let errors = thread::spawn(move || bounded_read(stderr, 16384));
    let start = Instant::now();
    let status = (|| {
        loop {
            checkpoint()?;
            if start.elapsed() >= deadline {
                return Err(EditorError::new(
                    "VIDEO_TIME_LIMIT",
                    "Video probe exceeded deadline",
                ));
            }
            if let Some(status) = process.child().try_wait().map_err(io)? {
                return Ok(status);
            }
            thread::sleep(Duration::from_millis(20));
        }
    })();
    drop(process);
    let bytes = out
        .join()
        .map_err(|_| err("Probe output reader failed"))?
        .map_err(io)?;
    let _ = errors.join();
    if !status?.success() || bytes.len() > 1048576 {
        return Err(err("Bounded video probe failed"));
    }
    serde_json::from_slice(&bytes).map_err(|_| err("Invalid observed video metadata"))
}
#[cfg(target_os = "linux")]
fn resident_bytes(pid: u32) -> Result<Option<u64>, EditorError> {
    let content = match std::fs::read_to_string(format!("/proc/{pid}/statm")) {
        Ok(v) => v,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io(e)),
    };
    let resident = content
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or_else(|| err("Encoder RSS observation unavailable"))?;
    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if page <= 0 {
        return Err(err("Encoder RSS page size unavailable"));
    }
    Ok(Some(resident.saturating_mul(page as u64)))
}
#[cfg(target_os = "macos")]
fn resident_bytes(pid: u32) -> Result<Option<u64>, EditorError> {
    #[repr(C)]
    struct TaskInfo {
        values: [u64; 6],
        counters: [i32; 12],
    }
    unsafe extern "C" {
        fn proc_pidinfo(
            pid: i32,
            flavor: i32,
            arg: u64,
            buffer: *mut std::ffi::c_void,
            size: i32,
        ) -> i32;
    }
    let mut info = TaskInfo {
        values: [0; 6],
        counters: [0; 12],
    };
    let size = std::mem::size_of::<TaskInfo>();
    let n = unsafe {
        proc_pidinfo(
            pid as i32,
            4,
            0,
            (&mut info as *mut TaskInfo).cast(),
            size as i32,
        )
    };
    if n == 0 {
        return Ok(None);
    }
    if n != size as i32 {
        return Err(err("Encoder RSS observation unavailable"));
    }
    Ok(Some(info.values[1]))
}
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn resident_bytes(_: u32) -> Result<Option<u64>, EditorError> {
    Err(EditorError::new(
        "CAPABILITY_UNAVAILABLE",
        "Video encoder RSS monitoring unavailable",
    ))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::process::Child;
    #[derive(Debug)]
    struct Admission;
    struct Process(Child);
    impl OutputProcess for Process {
        fn child(&mut self) -> &mut Child {
            &mut self.0
        }
        fn output_limit(&self) -> Option<u64> {
            Some(1048576)
        }
    }
    impl Drop for Process {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    impl OutputAdmission for Admission {
        fn spawn(
            &self,
            mut command: Command,
            _: Option<&Path>,
        ) -> opencut_media::MediaResult<Box<dyn OutputProcess>> {
            Ok(Box::new(Process(command.spawn()?)))
        }
        fn write(&self, _: &Path, _: &[u8]) -> opencut_media::MediaResult<()> {
            unreachable!()
        }
    }
    #[test]
    fn tool_resolution_requires_executable_regular_files_for_absolute_relative_and_path() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
        let executable = directory.path().join("encoder");
        std::fs::write(&executable, "fixture bytes need not execute").unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o600)).unwrap();
        let current = std::env::current_dir().unwrap();
        let relative = executable.strip_prefix(&current).unwrap().to_path_buf();
        let paths = std::env::join_paths([directory.path()]).unwrap();
        for rejected in [
            resolve_tool(executable.clone()),
            resolve_tool(relative.clone()),
            resolve_in_paths(Path::new("encoder"), &paths),
            resolve_tool(directory.path().into()),
        ] {
            assert_eq!(rejected.unwrap_err().code, "CAPABILITY_UNAVAILABLE");
        }
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        let expected = std::fs::canonicalize(&executable).unwrap();
        assert_eq!(resolve_tool(executable).unwrap(), expected);
        assert_eq!(resolve_tool(relative).unwrap(), expected);
        assert_eq!(
            resolve_in_paths(Path::new("encoder"), &paths).unwrap(),
            expected
        );
        let shadow = directory.path().join("shadow");
        std::fs::create_dir(&shadow).unwrap();
        std::fs::write(shadow.join("encoder"), "non executable shadow").unwrap();
        std::fs::set_permissions(
            shadow.join("encoder"),
            std::fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        let paths = std::env::join_paths([shadow.as_path(), directory.path()]).unwrap();
        assert_eq!(
            resolve_in_paths(Path::new("encoder"), &paths).unwrap(),
            expected
        );
    }
    #[test]
    fn encoder_backpressure_remains_cancellable_and_reaps_child() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("blocked-encoder");
        std::fs::write(&executable, "#!/bin/sh\nexec /bin/sleep 30\n").unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        let start = Instant::now();
        let cancel = opencut_media::CancellationToken::new();
        let mut encoder = ClassicVideoEncoder::start(
            ClassicVideoEncoderConfig {
                ffmpeg: executable,
                ffprobe: "/bin/sh".into(),
                output: directory.path().join("video.mp4"),
                audio: directory.path().join("audio.wav"),
                width: 256,
                height: 256,
                fps_numerator: 24,
                fps_denominator: 1,
                frame_count: 1,
                max_output_bytes: 1048576,
                max_rss_bytes: 67108864,
                deadline: Duration::from_secs(5),
            },
            &Admission,
            || {
                if cancel.is_cancelled() {
                    Err(EditorError::new(
                        "JOB_CANCELLED",
                        "Test cancelled during full pipe",
                    ))
                } else {
                    Ok(())
                }
            },
        )
        .unwrap();
        let pid = encoder.process.as_mut().unwrap().child().id();
        let trigger = cancel.clone();
        let cancellation = thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            trigger.cancel();
        });
        let mut failure = None;
        for index in 0..4 {
            if let Err(cause) = encoder.chunk(index * 65536, &vec![0u8; 65536]) {
                failure = Some(cause);
                break;
            }
        }
        cancellation.join().unwrap();
        assert_eq!(
            failure.expect("Unconsumed pipe must backpressure").code,
            "JOB_CANCELLED"
        );
        drop(encoder);
        assert!(start.elapsed() < Duration::from_secs(2));
        assert_eq!(unsafe { libc::kill(pid as i32, 0) }, -1);
    }
}
