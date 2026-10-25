use super::session_adapter::VisualProcessSession;
use crate::{browser_process::BrowserProcess, classic_audio::transport};
use opencut_contracts::EditorError;
use opencut_editor_application::classic_text_frame::*;
use opencut_editor_application::classic_visual::*;
use opencut_editor_application::media_frame_jobs::LocalVisualExecutionPolicy;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
pub struct ClassicVisualProcessConfig {
    pub node: PathBuf,
    pub runner: PathBuf,
    pub chromium: PathBuf,
    pub deadline: Duration,
    pub policy: LocalVisualExecutionPolicy,
    pub strict_cgroup: Option<PathBuf>,
}
pub struct ClassicVisualProcess {
    pub config: ClassicVisualProcessConfig,
}
fn error(code: &str, message: &str) -> EditorError {
    EditorError::new(code, message)
}
fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, EditorError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| error("VISUAL_PROTOCOL_ERROR", "Missing text field"))
}
fn number(value: &Value, key: &str) -> Result<u64, EditorError> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| error("VISUAL_PROTOCOL_ERROR", "Missing integer field"))
}
fn decimal(value: &str) -> Result<u64, EditorError> {
    let parsed = value
        .parse::<u64>()
        .map_err(|_| error("VISUAL_PROTOCOL_ERROR", "Invalid decimal integer"))?;
    if parsed.to_string() != value {
        return Err(error(
            "VISUAL_PROTOCOL_ERROR",
            "Noncanonical decimal integer",
        ));
    }
    Ok(parsed)
}
fn wire(error: EditorError) -> EditorError {
    if error.code.starts_with("AUDIO_") || error.code.starts_with("BROWSER_") {
        self::error("VISUAL_PROTOCOL_ERROR", "Isolated browser transport failed")
    } else {
        error
    }
}
impl ClassicVisualProcess {
    #[cfg(unix)]
    pub fn render(
        &self,
        session: &mut ClassicVisualSession<'_>,
        resources: &mut dyn ClassicVisualResources,
        handles: &[ClassicVisualResourceHandle],
        sink: &mut dyn ClassicVisualRgbaSink,
    ) -> Result<ClassicVisualExecutionReceipt, EditorError> {
        self.render_session(
            session,
            resources,
            "begin",
            serde_json::to_value(handles)
                .map_err(|_| error("INVALID_VISUAL_PLAN", "Invalid resource handles"))?,
            None,
            sink,
        )
    }
    #[cfg(unix)]
    pub(crate) fn render_session<R: ClassicVisualResources + ?Sized, S: VisualProcessSession<R>>(
        &self,
        session: &mut S,
        resources: &mut R,
        begin_kind: &str,
        handles: Value,
        fonts: Option<Value>,
        sink: &mut dyn ClassicVisualRgbaSink,
    ) -> Result<S::Output, EditorError> {
        let config = &self.config;
        let policy = &config.policy;
        if config.deadline.is_zero()
            || config.deadline > Duration::from_secs(300)
            || config.deadline.as_millis() != u128::from(policy.deadline_ms)
            || session.policy() != policy
        {
            return Err(error(
                "INVALID_VISUAL_PLAN",
                "Runtime admission differs from captured visual policy",
            ));
        }
        resources.checkpoint()?;
        let execution = uuid::Uuid::new_v4().to_string();
        let plan = session.plan()?;
        let mut process =
            BrowserProcess::start(&config.node, &config.runner, config.deadline).map_err(wire)?;
        let mut input = process
            .input
            .take()
            .ok_or_else(|| error("VISUAL_HOST_FAILED", "Missing runner input"))?;
        let pid = process.guard.child.id();
        let started = Instant::now();
        let mut begin = json!({"type":begin_kind,"plan":plan,"resources":handles,"runtime":{"chromiumExecutable":config.chromium},"admission":{
            "maxFramePixels":policy.max_frame_pixels.to_string(),"maxDecodedPixels":policy.max_decoded_pixels.to_string(),
            "maxGpuBytes":policy.max_gpu_bytes.to_string(),"maxRgbaBytes":policy.max_rgba_bytes.to_string(),
            "maxEncodedBytes":policy.max_encoded_bytes,"maxOutputBytes":policy.max_output_bytes.to_string(),
            "maxRssBytes":policy.max_rss_bytes.to_string(),"deadlineMs":policy.deadline_ms,
            "quotaProfile":if config.strict_cgroup.is_some(){"strict-hard"}else{"monitored-terminate"},"cgroupDirectory":config.strict_cgroup}});
        if let Some(fonts) = fonts {
            begin["fonts"] = fonts;
        }
        let result = (|| {
            transport::packet(&mut input, &execution, begin).map_err(wire)?;
            let mut runtime = None;
            let mut requests = std::collections::HashSet::new();
            loop {
                resources.checkpoint()?;
                if started.elapsed() >= config.deadline {
                    return Err(error(
                        "VISUAL_TIME_LIMIT",
                        "Visual execution exceeded deadline",
                    ));
                }
                let Some(message) = process.receive().map_err(wire)? else {
                    continue;
                };
                if message.get("executionId").and_then(Value::as_str) != Some(&execution) {
                    return Err(error("VISUAL_PROTOCOL_ERROR", "Execution envelope changed"));
                }
                match text(&message, "type")? {
                    "started" => {
                        let browser = number(&message, "browserPid")?;
                        if browser == 0
                            || browser == u64::from(pid)
                            || browser > i32::MAX as u64
                            || process.guard.browser_pid.load(Ordering::Acquire) != 0
                        {
                            return Err(error("VISUAL_PROTOCOL_ERROR", "Invalid browser PID"));
                        }
                        process
                            .guard
                            .browser_pid
                            .store(browser as u32, Ordering::Release);
                    }
                    "ready" => {
                        if runtime.is_some()
                            || number(&message, "browserPid")?
                                != u64::from(process.guard.browser_pid.load(Ordering::Acquire))
                            || process.guard.browser_pid.load(Ordering::Acquire) == 0
                        {
                            return Err(error(
                                "VISUAL_PROTOCOL_ERROR",
                                "Invalid browser readiness",
                            ));
                        }
                        let identity: ClassicVisualEngineIdentity =
                            serde_json::from_value(message.get("identity").cloned().ok_or_else(
                                || error("VISUAL_PROTOCOL_ERROR", "Missing runtime identity"),
                            )?)
                            .map_err(|_| {
                                error("VISUAL_PROTOCOL_ERROR", "Invalid typed engine identity")
                            })?;
                        validate_identity(&identity)?;
                        runtime = Some(identity);
                    }
                    "request" => {
                        if runtime.is_none() {
                            return Err(error(
                                "VISUAL_PROTOCOL_ERROR",
                                "Callback before readiness",
                            ));
                        }
                        let id = text(&message, "id")?.to_string();
                        if id.is_empty()
                            || id.len() > 128
                            || requests.len() >= 100000
                            || !requests.insert(id.clone())
                        {
                            return Err(error(
                                "VISUAL_PROTOCOL_ERROR",
                                "Invalid or reused callback ID",
                            ));
                        }
                        let callback: S::Callback = serde_json::from_value(
                            json!({"op":message.get("op"),"args":message.get("args")}),
                        )
                        .map_err(|_| {
                            error("VISUAL_PROTOCOL_ERROR", "Invalid typed visual callback")
                        })?;
                        match session.callback(callback, resources) {
                            Ok(value) => transport::reply(&mut input, &execution, &id, value)
                                .map_err(wire)?,
                            Err(cause) => {
                                transport::packet(&mut input,&execution,json!({"type":"reply","id":id,"ok":false,"error":{"code":cause.code,"message":cause.message}})).map_err(wire)?;
                                return Err(cause);
                            }
                        }
                    }
                    "rgba" => {
                        if runtime.is_none() {
                            return Err(error("VISUAL_PROTOCOL_ERROR", "RGBA before readiness"));
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
                            return Err(error(
                                "VISUAL_PROTOCOL_ERROR",
                                "Invalid bounded RGBA encoding",
                            ));
                        }
                        let bytes = transport::decode_hex(hex).map_err(wire)?;
                        session.rgba_chunk(offset, &bytes, sink)?;
                    }
                    "complete" => {
                        let identity = runtime.take().ok_or_else(|| {
                            error("VISUAL_PROTOCOL_ERROR", "Completion before readiness")
                        })?;
                        let receipt: S::Receipt =
                            serde_json::from_value(message.get("receipt").cloned().ok_or_else(
                                || error("VISUAL_PROTOCOL_ERROR", "Missing frame receipt"),
                            )?)
                            .map_err(|_| {
                                error("VISUAL_PROTOCOL_ERROR", "Invalid typed frame receipt")
                            })?;
                        let quota: VisualQuotaReceipt =
                            serde_json::from_value(message.get("quota").cloned().ok_or_else(
                                || error("VISUAL_PROTOCOL_ERROR", "Missing quota receipt"),
                            )?)
                            .map_err(|_| {
                                error("VISUAL_PROTOCOL_ERROR", "Invalid typed quota receipt")
                            })?;
                        validate_quota(&quota, config, Some(config.deadline))?;
                        return session.finish(
                            receipt,
                            quota,
                            ClassicVisualRuntimeReceipt {
                                browser_pid: process.guard.browser_pid.load(Ordering::Acquire),
                                identity,
                            },
                        );
                    }
                    "failed" => {
                        return Err(error(
                            message
                                .pointer("/error/code")
                                .and_then(Value::as_str)
                                .unwrap_or("VISUAL_HOST_FAILED"),
                            "Isolated Classic visual execution failed",
                        ));
                    }
                    "cancelled" => {
                        return Err(error(
                            "JOB_CANCELLED",
                            "Isolated visual execution acknowledged cancellation",
                        ));
                    }
                    _ => return Err(error("VISUAL_PROTOCOL_ERROR", "Unknown visual packet")),
                }
            }
        })();
        if result.is_err() {
            let _ = transport::packet(&mut input, &execution, json!({"type":"cancel"}));
        }
        drop(input);
        process.finish(result.is_ok(), &execution).map_err(wire)?;
        if result.is_ok() {
            resources.checkpoint()?;
        }
        result
    }
}
pub(crate) fn validate_identity(identity: &ClassicVisualEngineIdentity) -> Result<(), EditorError> {
    let hash = |value: &str| {
        value.len() == 64
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    };
    if identity.engine.is_empty()
        || identity.browser_version.is_empty()
        || identity.gpu_device.is_empty()
        || identity.gpu_backend != "webgpu"
        || identity.texture_format != "bgra8unorm"
        || identity.decoder_profile
            != "mediabunny-1.41.0-initial-canvases-half-open;static-png-jpeg-browser-image"
        || identity.playwright_version != "1.63.0"
        || identity.mediabunny_version != "1.41.0"
        || !hash(&identity.browser_sha256)
        || !hash(&identity.module_sha256)
        || !hash(&identity.shader_sha256)
    {
        return Err(error(
            "VISUAL_ENGINE_CHANGED",
            "Runtime lacks pinned actual visual identity",
        ));
    }
    Ok(())
}
pub(crate) fn validate_quota(
    quota: &VisualQuotaReceipt,
    config: &ClassicVisualProcessConfig,
    elapsed_budget: Option<Duration>,
) -> Result<(), EditorError> {
    let strict = config.strict_cgroup.is_some();
    if quota.profile
        != if strict {
            "strict-hard"
        } else {
            "monitored-terminate"
        }
        || decimal(&quota.max_rss_bytes)? != config.policy.max_rss_bytes
        || decimal(&quota.observed_peak_rss_bytes)? > config.policy.max_rss_bytes
        || quota.hard_memory_enforced != strict
        || !(20..=250).contains(&quota.monitor_interval_ms)
        || elapsed_budget.is_some_and(|budget| u128::from(quota.elapsed_ms) > budget.as_millis())
    {
        return Err(error(
            "VISUAL_PROTOCOL_ERROR",
            "Quota receipt differs from host admission",
        ));
    }
    Ok(())
}
impl ClassicVisualExecutor for ClassicVisualProcess {
    fn execute(
        &self,
        session: &mut ClassicVisualSession<'_>,
        resources: &mut dyn ClassicVisualResources,
        handles: &[ClassicVisualResourceHandle],
        sink: &mut dyn ClassicVisualRgbaSink,
    ) -> Result<ClassicVisualExecutionReceipt, EditorError> {
        #[cfg(unix)]
        {
            self.render(session, resources, handles, sink)
        }
        #[cfg(not(unix))]
        {
            let _ = (session, resources, handles, sink);
            Err(error(
                "CAPABILITY_UNAVAILABLE",
                "Isolated visual execution requires process-group termination",
            ))
        }
    }
}

impl ClassicTextFrameExecutor for ClassicVisualProcess {
    fn execute_text_frame(
        &self,
        session: &mut ClassicTextFrameSession<'_>,
        resources: &mut dyn ClassicTextFrameResources,
        handles: &[ClassicVisualResourceHandle],
        fonts: &[ClassicTextFontHandle],
        sink: &mut dyn ClassicVisualRgbaSink,
    ) -> Result<ClassicTextFrameExecution, EditorError> {
        #[cfg(unix)]
        {
            self.render_session(
                session,
                resources,
                "beginTextFrame",
                serde_json::to_value(handles)
                    .map_err(|_| error("INVALID_VISUAL_PLAN", "Invalid media handles"))?,
                Some(
                    serde_json::to_value(fonts)
                        .map_err(|_| error("INVALID_VISUAL_PLAN", "Invalid font handles"))?,
                ),
                sink,
            )
        }
        #[cfg(not(unix))]
        {
            let _ = (session, resources, handles, fonts, sink);
            Err(error(
                "CAPABILITY_UNAVAILABLE",
                "Isolated text execution requires process-group termination",
            ))
        }
    }
}
