use crate::classic_visual::VisualProcessSession;
use crate::{
    browser_process::BrowserProcess,
    classic_audio::transport,
    classic_visual::{ClassicVisualProcessConfig, validate_identity, validate_quota},
};
use opencut_contracts::EditorError;
use opencut_editor_application::media_video_jobs::{
    MAX_VIDEO_DEADLINE_MS, MAX_VIDEO_FRAMES, VideoExecutionLimit,
};
use opencut_editor_application::{
    classic_blend_frame::*, classic_caption_frame::*, classic_graphic_frame::*,
    classic_mask_frame::*, classic_scene_frame::*, classic_text_frame::*, classic_visual::*,
};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    process::ChildStdin,
    sync::{Arc, Mutex, atomic::Ordering},
    time::{Duration, Instant},
};
trait VideoReceiptValidation<R> {
    fn validate_video_receipt(&self, receipt: &R) -> Result<(), EditorError>;
}
impl VideoReceiptValidation<ClassicTextFrameReceipt> for ClassicTextFrameSession<'_> {
    fn validate_video_receipt(&self, receipt: &ClassicTextFrameReceipt) -> Result<(), EditorError> {
        self.validate_receipt(receipt)
    }
}
impl VideoReceiptValidation<ClassicCaptionFrameReceipt> for ClassicCaptionFrameSession<'_> {
    fn validate_video_receipt(
        &self,
        receipt: &ClassicCaptionFrameReceipt,
    ) -> Result<(), EditorError> {
        self.validate_receipt(receipt)
    }
}
impl VideoReceiptValidation<ClassicGraphicFrameReceipt> for ClassicGraphicFrameSession<'_> {
    fn validate_video_receipt(
        &self,
        receipt: &ClassicGraphicFrameReceipt,
    ) -> Result<(), EditorError> {
        self.validate_receipt(receipt)
    }
}
impl VideoReceiptValidation<ClassicMaskFrameReceipt> for ClassicMaskFrameSession<'_> {
    fn validate_video_receipt(&self, receipt: &ClassicMaskFrameReceipt) -> Result<(), EditorError> {
        self.validate_receipt(receipt)
    }
}
impl VideoReceiptValidation<ClassicSceneFrameReceipt> for ClassicSceneFrameSession<'_> {
    fn validate_video_receipt(
        &self,
        receipt: &ClassicSceneFrameReceipt,
    ) -> Result<(), EditorError> {
        self.validate_receipt(receipt)
    }
}
impl VideoReceiptValidation<ClassicBlendFrameReceipt> for ClassicBlendFrameSession<'_> {
    fn validate_video_receipt(
        &self,
        receipt: &ClassicBlendFrameReceipt,
    ) -> Result<(), EditorError> {
        self.validate_receipt(receipt)
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum VideoFrameMode {
    TextV2,
    CaptionV3,
    GraphicV1,
    BlendV1,
    MaskV1,
    SceneV1,
}
impl VideoFrameMode {
    fn begin(self) -> &'static str {
        match self {
            Self::TextV2 => "beginTextVideo",
            Self::CaptionV3 => "beginCaptionVideo",
            Self::GraphicV1 => "beginGraphicVideo",
            Self::MaskV1 => "beginMaskVideo",
            Self::SceneV1 => "beginSceneVideo",
            Self::BlendV1 => "beginBlendVideo",
        }
    }
}
fn err(message: &str) -> EditorError {
    EditorError::new("VIDEO_PROTOCOL_ERROR", message)
}
fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str, EditorError> {
    v.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| err("Missing video text field"))
}
fn number(v: &Value, key: &str) -> Result<u64, EditorError> {
    v.get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| err("Missing video integer field"))
}
fn decimal(v: &str) -> Result<u64, EditorError> {
    let n = v.parse::<u64>().map_err(|_| err("Invalid video decimal"))?;
    if n.to_string() != v {
        return Err(err("Noncanonical video decimal"));
    }
    Ok(n)
}
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasVideoEncoding {
    pub fps_numerator: u32,
    pub fps_denominator: u32,
    pub bitrate: u64,
    pub max_output_bytes: u64,
}
/// Owns exactly one isolated browser; each frame must finish and validate before
/// the caller may send the next. No frame queue or full-scene RGBA allocation.
pub struct ClassicVideoProcess {
    process: Option<BrowserProcess>,
    input: Option<ChildStdin>,
    config: ClassicVisualProcessConfig,
    execution: String,
    started: Instant,
    operation_started: Instant,
    execution_limit: VideoExecutionLimit,
    progress_clock: Option<Arc<Mutex<Instant>>>,
    next: u64,
    count: u64,
    runtime: Option<ClassicVisualRuntimeReceipt>,
    mode: VideoFrameMode,
    canvas_encoding: Option<CanvasVideoEncoding>,
}
impl ClassicVideoProcess {
    #[cfg(unix)]
    pub fn start(
        config: ClassicVisualProcessConfig,
        count: u64,
        execution_limit: VideoExecutionLimit,
        progress_clock: Option<Arc<Mutex<Instant>>>,
        checkpoint: &mut dyn FnMut() -> Result<(), EditorError>,
    ) -> Result<Self, EditorError> {
        Self::start_mode(
            config,
            count,
            execution_limit,
            progress_clock,
            checkpoint,
            VideoFrameMode::TextV2,
            None,
        )
    }
    #[cfg(unix)]
    pub fn start_caption(
        config: ClassicVisualProcessConfig,
        count: u64,
        execution_limit: VideoExecutionLimit,
        progress_clock: Option<Arc<Mutex<Instant>>>,
        checkpoint: &mut dyn FnMut() -> Result<(), EditorError>,
    ) -> Result<Self, EditorError> {
        Self::start_mode(
            config,
            count,
            execution_limit,
            progress_clock,
            checkpoint,
            VideoFrameMode::CaptionV3,
            None,
        )
    }
    #[cfg(unix)]
    pub fn start_graphic(
        config: ClassicVisualProcessConfig,
        count: u64,
        execution_limit: VideoExecutionLimit,
        progress_clock: Option<Arc<Mutex<Instant>>>,
        checkpoint: &mut dyn FnMut() -> Result<(), EditorError>,
    ) -> Result<Self, EditorError> {
        Self::start_mode(
            config,
            count,
            execution_limit,
            progress_clock,
            checkpoint,
            VideoFrameMode::GraphicV1,
            None,
        )
    }
    pub fn start_mask(
        config: ClassicVisualProcessConfig,
        count: u64,
        execution_limit: VideoExecutionLimit,
        progress_clock: Option<Arc<Mutex<Instant>>>,
        checkpoint: &mut dyn FnMut() -> Result<(), EditorError>,
    ) -> Result<Self, EditorError> {
        Self::start_mode(
            config,
            count,
            execution_limit,
            progress_clock,
            checkpoint,
            VideoFrameMode::MaskV1,
            None,
        )
    }
    pub fn start_scene(
        config: ClassicVisualProcessConfig,
        count: u64,
        execution_limit: VideoExecutionLimit,
        progress_clock: Option<Arc<Mutex<Instant>>>,
        checkpoint: &mut dyn FnMut() -> Result<(), EditorError>,
    ) -> Result<Self, EditorError> {
        Self::start_mode(
            config,
            count,
            execution_limit,
            progress_clock,
            checkpoint,
            VideoFrameMode::SceneV1,
            None,
        )
    }
    pub fn start_blend(
        config: ClassicVisualProcessConfig,
        count: u64,
        execution_limit: VideoExecutionLimit,
        progress_clock: Option<Arc<Mutex<Instant>>>,
        checkpoint: &mut dyn FnMut() -> Result<(), EditorError>,
    ) -> Result<Self, EditorError> {
        Self::start_mode(
            config,
            count,
            execution_limit,
            progress_clock,
            checkpoint,
            VideoFrameMode::BlendV1,
            None,
        )
    }
    #[cfg(unix)]
    pub fn start_scene_canvas(
        config: ClassicVisualProcessConfig,
        count: u64,
        limit: VideoExecutionLimit,
        clock: Option<Arc<Mutex<Instant>>>,
        checkpoint: &mut dyn FnMut() -> Result<(), EditorError>,
        encoding: CanvasVideoEncoding,
    ) -> Result<Self, EditorError> {
        Self::start_mode(
            config,
            count,
            limit,
            clock,
            checkpoint,
            VideoFrameMode::SceneV1,
            Some(encoding),
        )
    }
    #[cfg(unix)]
    fn start_mode(
        config: ClassicVisualProcessConfig,
        count: u64,
        execution_limit: VideoExecutionLimit,
        progress_clock: Option<Arc<Mutex<Instant>>>,
        checkpoint: &mut dyn FnMut() -> Result<(), EditorError>,
        mode: VideoFrameMode,
        canvas_encoding: Option<CanvasVideoEncoding>,
    ) -> Result<Self, EditorError> {
        if count == 0
            || count > MAX_VIDEO_FRAMES
            || execution_limit.renews_on_progress() != progress_clock.is_some()
            || execution_limit.milliseconds() == 0
            || execution_limit.milliseconds() > MAX_VIDEO_DEADLINE_MS
            || config.deadline.is_zero()
            || config.deadline.as_millis() != u128::from(config.policy.deadline_ms)
        {
            return Err(err("Video admission differs from captured policy"));
        }
        if canvas_encoding.as_ref().is_some_and(|encoding| {
            !matches!(mode, VideoFrameMode::SceneV1)
                || encoding.fps_numerator == 0
                || encoding.fps_denominator == 0
                || encoding.bitrate == 0
                || encoding.bitrate > u64::from(u32::MAX)
                || encoding.max_output_bytes == 0
                || encoding.max_output_bytes > 8 * 1024 * 1024
        }) {
            return Err(err("Invalid bounded canvas video admission"));
        }
        checkpoint()?;
        let mut process = BrowserProcess::start(
            &config.node,
            &config.runner,
            Duration::from_millis(execution_limit.milliseconds()),
        )?;
        if let Some(clock) = &progress_clock {
            process.use_progress_timeout(clock.clone());
        }
        let input = process
            .input
            .take()
            .ok_or_else(|| err("Missing video runner input"))?;
        let execution = uuid::Uuid::new_v4().to_string();
        let mut this = Self {
            process: Some(process),
            input: Some(input),
            config,
            execution,
            started: Instant::now(),
            operation_started: Instant::now(),
            execution_limit,
            progress_clock,
            next: 0,
            count,
            runtime: None,
            mode,
            canvas_encoding,
        };
        let policy = &this.config.policy;
        let begin = json!({"type":mode.begin(),"frameCount":count,"executionLimit":execution_limit,"canvasEncoding":this.canvas_encoding,"runtime":{"chromiumExecutable":this.config.chromium},"admission":{
            "maxFramePixels":policy.max_frame_pixels.to_string(),"maxDecodedPixels":policy.max_decoded_pixels.to_string(),"maxGpuBytes":policy.max_gpu_bytes.to_string(),"maxRgbaBytes":policy.max_rgba_bytes.to_string(),"maxEncodedBytes":policy.max_encoded_bytes,"maxOutputBytes":policy.max_output_bytes.to_string(),"maxRssBytes":policy.max_rss_bytes.to_string(),"deadlineMs":policy.deadline_ms,"quotaProfile":if this.config.strict_cgroup.is_some(){"strict-hard"}else{"monitored-terminate"},"cgroupDirectory":this.config.strict_cgroup}});
        this.send(begin)?;
        loop {
            checkpoint()?;
            let Some(message) = this.receive()? else {
                continue;
            };
            match text(&message, "type")? {
                "started" => {
                    let pid = number(&message, "browserPid")?;
                    let process = this.process.as_ref().unwrap();
                    if pid == 0
                        || pid > i32::MAX as u64
                        || pid == u64::from(process.guard.child.id())
                        || process.guard.browser_pid.load(Ordering::Acquire) != 0
                    {
                        return Err(err("Invalid persistent browser PID"));
                    }
                    process
                        .guard
                        .browser_pid
                        .store(pid as u32, Ordering::Release);
                }
                "ready" => {
                    let process = this.process.as_ref().unwrap();
                    let pid = process.guard.browser_pid.load(Ordering::Acquire);
                    if pid == 0 || number(&message, "browserPid")? != u64::from(pid) {
                        return Err(err("Persistent browser readiness changed"));
                    }
                    let identity: ClassicVisualEngineIdentity = serde_json::from_value(
                        message
                            .get("identity")
                            .cloned()
                            .ok_or_else(|| err("Missing video runtime"))?,
                    )
                    .map_err(|_| err("Invalid video runtime"))?;
                    validate_identity(&identity)?;
                    this.runtime = Some(ClassicVisualRuntimeReceipt {
                        browser_pid: pid,
                        identity,
                    });
                    return Ok(this);
                }
                "failed" | "cancelled" => return Err(failed(&message)),
                _ => return Err(err("Unexpected video startup packet")),
            }
        }
    }
    fn send(&mut self, value: Value) -> Result<(), EditorError> {
        transport::packet(
            self.input
                .as_mut()
                .ok_or_else(|| err("Closed video input"))?,
            &self.execution,
            value,
        )
    }
    fn receive(&self) -> Result<Option<Value>, EditorError> {
        let elapsed = if let Some(clock) = &self.progress_clock {
            clock.lock().unwrap_or_else(|e| e.into_inner()).elapsed()
        } else {
            self.started.elapsed()
        };
        if elapsed >= Duration::from_millis(self.execution_limit.milliseconds())
            || self.operation_started.elapsed() >= self.config.deadline
        {
            return Err(EditorError::new(
                "VIDEO_TIME_LIMIT",
                "Video task or active frame exceeded its captured deadline",
            ));
        }
        let Some(message) = self
            .process
            .as_ref()
            .ok_or_else(|| err("Closed video process"))?
            .receive()?
        else {
            return Ok(None);
        };
        if message.get("executionId").and_then(Value::as_str) != Some(&self.execution) {
            return Err(err("Video envelope changed"));
        }
        Ok(Some(message))
    }
    pub fn runtime(&self) -> Result<&ClassicVisualRuntimeReceipt, EditorError> {
        self.runtime
            .as_ref()
            .ok_or_else(|| err("Missing persistent runtime"))
    }
    #[cfg(unix)]
    pub fn render_frame(
        &mut self,
        session: &mut ClassicTextFrameSession<'_>,
        resources: &mut dyn ClassicTextFrameResources,
        handles: &[ClassicVisualResourceHandle],
        fonts: &[ClassicTextFontHandle],
        sink: &mut dyn ClassicVisualRgbaSink,
    ) -> Result<ClassicTextFrameReceipt, EditorError> {
        if self.mode != VideoFrameMode::TextV2 {
            return Err(err("Text frame differs from selected video execution"));
        }
        self.render_session(session, resources, handles, fonts, sink)
    }
    #[cfg(unix)]
    pub fn render_caption_frame(
        &mut self,
        session: &mut ClassicCaptionFrameSession<'_>,
        resources: &mut dyn ClassicTextFrameResources,
        handles: &[ClassicVisualResourceHandle],
        fonts: &[ClassicTextFontHandle],
        sink: &mut dyn ClassicVisualRgbaSink,
    ) -> Result<ClassicCaptionFrameReceipt, EditorError> {
        if self.mode != VideoFrameMode::CaptionV3 {
            return Err(err("Caption frame differs from selected video execution"));
        }
        self.render_session(session, resources, handles, fonts, sink)
    }
    #[cfg(unix)]
    pub fn render_graphic_frame(
        &mut self,
        session: &mut ClassicGraphicFrameSession<'_>,
        resources: &mut dyn ClassicTextFrameResources,
        handles: &[ClassicVisualResourceHandle],
        fonts: &[ClassicTextFontHandle],
        sink: &mut dyn ClassicVisualRgbaSink,
    ) -> Result<ClassicGraphicFrameReceipt, EditorError> {
        if self.mode != VideoFrameMode::GraphicV1 {
            return Err(err("Graphic frame differs from selected video execution"));
        }
        self.render_session(session, resources, handles, fonts, sink)
    }
    pub fn render_mask_frame(
        &mut self,
        session: &mut ClassicMaskFrameSession<'_>,
        resources: &mut dyn ClassicTextFrameResources,
        handles: &[ClassicVisualResourceHandle],
        fonts: &[ClassicTextFontHandle],
        sink: &mut dyn ClassicVisualRgbaSink,
    ) -> Result<ClassicMaskFrameReceipt, EditorError> {
        if self.mode != VideoFrameMode::MaskV1 {
            return Err(err("Mask frame differs from selected video execution"));
        }
        self.render_session(session, resources, handles, fonts, sink)
    }
    pub fn render_scene_frame(
        &mut self,
        session: &mut ClassicSceneFrameSession<'_>,
        resources: &mut dyn ClassicTextFrameResources,
        handles: &[ClassicVisualResourceHandle],
        fonts: &[ClassicTextFontHandle],
        sink: &mut dyn ClassicVisualRgbaSink,
    ) -> Result<ClassicSceneFrameReceipt, EditorError> {
        if self.mode != VideoFrameMode::SceneV1 {
            return Err(err("Scene frame differs from selected video execution"));
        }
        self.render_session(session, resources, handles, fonts, sink)
    }
    pub fn render_blend_frame(
        &mut self,
        session: &mut ClassicBlendFrameSession<'_>,
        resources: &mut dyn ClassicTextFrameResources,
        handles: &[ClassicVisualResourceHandle],
        fonts: &[ClassicTextFontHandle],
        sink: &mut dyn ClassicVisualRgbaSink,
    ) -> Result<ClassicBlendFrameReceipt, EditorError> {
        if self.mode != VideoFrameMode::BlendV1 {
            return Err(err("Blend frame differs from selected video execution"));
        }
        self.render_session(session, resources, handles, fonts, sink)
    }
    #[cfg(unix)]
    fn render_session<S, R>(
        &mut self,
        session: &mut S,
        resources: &mut R,
        handles: &[ClassicVisualResourceHandle],
        fonts: &[ClassicTextFontHandle],
        sink: &mut dyn ClassicVisualRgbaSink,
    ) -> Result<S::Receipt, EditorError>
    where
        R: ClassicTextFrameResources + ?Sized,
        S: VisualProcessSession<R>
            + VideoReceiptValidation<<S as VisualProcessSession<R>>::Receipt>,
    {
        if self.next >= self.count || session.policy() != &self.config.policy {
            return Err(err("Frame differs from persistent admission"));
        }
        self.operation_started = Instant::now();
        resources.checkpoint()?;
        self.send(json!({"type":"frame","frameIndex":self.next,"plan":session.plan()?,"resources":handles,"fonts":fonts}))?;
        let mut requests = HashSet::new();
        loop {
            resources.checkpoint()?;
            let Some(message) = self.receive()? else {
                continue;
            };
            let kind = text(&message, "type")?;
            if kind == "failed" || kind == "cancelled" {
                return Err(failed(&message));
            }
            if number(&message, "frameIndex")? != self.next {
                return Err(err("Packet belongs to another video frame"));
            }
            match kind {
                "request" => {
                    let id = text(&message, "id")?.to_owned();
                    if id.is_empty()
                        || id.len() > 128
                        || requests.len() >= 100000
                        || !requests.insert(id.clone())
                    {
                        return Err(err("Reused or excessive video callback"));
                    }
                    let request: S::Callback = serde_json::from_value(
                        json!({"op":message.get("op"),"args":message.get("args")}),
                    )
                    .map_err(|_| err("Invalid versioned video callback"))?;
                    let value = session.callback(request, resources)?;
                    transport::reply(self.input.as_mut().unwrap(), &self.execution, &id, value)?;
                }
                "rgba" => {
                    if self.canvas_encoding.is_some() {
                        return Err(err("Canvas execution cannot emit RGBA"));
                    }
                    let offset = decimal(text(&message, "offsetBytes")?)?;
                    let hex = text(&message, "bytesHex")?;
                    if hex.is_empty()
                        || hex.len() > 131072
                        || !hex.len().is_multiple_of(8)
                        || !hex
                            .bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                    {
                        return Err(err("Invalid bounded video RGBA"));
                    }
                    let bytes = transport::decode_hex(hex)?;
                    session.rgba_chunk(offset, &bytes, sink)?;
                }
                "frameComplete" => {
                    let receipt: S::Receipt = serde_json::from_value(
                        message
                            .get("receipt")
                            .cloned()
                            .ok_or_else(|| err("Missing versioned frame receipt"))?,
                    )
                    .map_err(|_| err("Invalid versioned frame receipt"))?;
                    session.validate_video_receipt(&receipt)?;
                    resources.checkpoint()?;
                    self.next += 1;
                    return Ok(receipt);
                }
                _ => return Err(err("Unexpected persistent frame packet")),
            }
        }
    }
    #[cfg(unix)]
    pub fn finish(
        self,
        checkpoint: &mut dyn FnMut() -> Result<(), EditorError>,
    ) -> Result<(ClassicVisualRuntimeReceipt, VisualQuotaReceipt), EditorError> {
        self.finish_output(checkpoint, None)
    }
    #[cfg(unix)]
    pub fn finish_canvas(
        self,
        checkpoint: &mut dyn FnMut() -> Result<(), EditorError>,
        sink: &mut dyn FnMut(u64, &[u8]) -> Result<(), EditorError>,
    ) -> Result<(ClassicVisualRuntimeReceipt, VisualQuotaReceipt), EditorError> {
        if self.canvas_encoding.is_none() {
            return Err(err("Missing canvas encoder admission"));
        }
        self.finish_output(checkpoint, Some(sink))
    }
    #[cfg(unix)]
    fn finish_output(
        mut self,
        checkpoint: &mut dyn FnMut() -> Result<(), EditorError>,
        mut sink: Option<&mut dyn FnMut(u64, &[u8]) -> Result<(), EditorError>>,
    ) -> Result<(ClassicVisualRuntimeReceipt, VisualQuotaReceipt), EditorError> {
        if self.next != self.count {
            return Err(err("Persistent execution lacks all frames"));
        }
        self.operation_started = Instant::now();
        checkpoint()?;
        self.send(json!({"type":"endVideo"}))?;
        let mut encoded_received = 0u64;
        let quota = loop {
            checkpoint()?;
            let Some(message) = self.receive()? else {
                continue;
            };
            match text(&message, "type")? {
                "encoded" => {
                    let encoding = self
                        .canvas_encoding
                        .as_ref()
                        .ok_or_else(|| err("Unselected encoded output"))?;
                    let offset = decimal(text(&message, "offsetBytes")?)?;
                    let hex = text(&message, "bytesHex")?;
                    if hex.is_empty() || hex.len() > 131072 || offset != encoded_received {
                        return Err(err("Invalid bounded encoded video packet"));
                    }
                    let bytes = transport::decode_hex(hex)?;
                    encoded_received = encoded_received
                        .checked_add(bytes.len() as u64)
                        .ok_or_else(|| err("Encoded size overflow"))?;
                    if encoded_received > encoding.max_output_bytes {
                        return Err(err("Canvas output exceeded admitted bytes"));
                    }
                    sink.as_deref_mut()
                        .ok_or_else(|| err("Missing owned encoded output sink"))?(
                        offset, &bytes
                    )?;
                }
                "complete" => {
                    if self.canvas_encoding.is_some()
                        && (encoded_received == 0
                            || number(&message, "encodedBytes")? != encoded_received)
                    {
                        return Err(err("Incomplete canvas video output"));
                    }
                    if number(&message, "frameCount")? != self.count {
                        return Err(err("Video completion count changed"));
                    }
                    let quota: VisualQuotaReceipt = serde_json::from_value(
                        message
                            .get("quota")
                            .cloned()
                            .ok_or_else(|| err("Missing video quota"))?,
                    )
                    .map_err(|_| err("Invalid video quota"))?;
                    validate_quota(
                        &quota,
                        &self.config,
                        if self.execution_limit.renews_on_progress() {
                            None
                        } else {
                            Some(Duration::from_millis(self.execution_limit.milliseconds()))
                        },
                    )?;
                    break quota;
                }
                "failed" | "cancelled" => return Err(failed(&message)),
                _ => return Err(err("Unexpected video terminal packet")),
            }
        };
        drop(self.input.take());
        self.process.take().unwrap().finish(true, &self.execution)?;
        checkpoint()?;
        Ok((self.runtime.take().unwrap(), quota))
    }
}
impl Drop for ClassicVideoProcess {
    fn drop(&mut self) {
        if self.process.is_some() {
            let _ = self.send(json!({"type":"cancel"}));
            drop(self.input.take());
        }
    }
}
fn failed(message: &Value) -> EditorError {
    if message.get("type").and_then(Value::as_str) == Some("cancelled") {
        return EditorError::new(
            "JOB_CANCELLED",
            "Isolated persistent video acknowledged cancellation",
        );
    }
    EditorError::new(
        message
            .pointer("/error/code")
            .and_then(Value::as_str)
            .unwrap_or("VIDEO_HOST_FAILED"),
        "Isolated persistent video execution failed",
    )
}
