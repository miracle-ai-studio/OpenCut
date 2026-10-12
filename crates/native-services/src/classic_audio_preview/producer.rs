mod receipt;
use crate::{browser_process::BrowserProcess, classic_audio::transport::*};
use opencut_contracts::EditorError;
use opencut_editor_application::classic_audio::*;
use serde_json::{Value, json};
use std::{
    process::ChildStdin,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

use super::{
    ClassicAudioPreviewPcmBlock, ClassicAudioPreviewProcessConfig, new_identity, protocol::*,
};

pub struct ClassicAudioPreviewProducer {
    process: Option<BrowserProcess>,
    input: Option<ChildStdin>,
    execution: String,
    started: Instant,
    next_id: u64,
    last_callback_id: u64,
    admitted: serde_json::Map<String, Value>,
    bound_sources: Option<Value>,
    resource_ids: Vec<String>,
    voice_ids: Vec<String>,
    fixed_voices: Value,
    runtime: Option<Value>,
    prepared: Option<Value>,
    identity: Value,
    max_pcm_bytes: u64,
    max_rss_bytes: u64,
    strict: bool,
}

impl Drop for ClassicAudioPreviewProducer {
    fn drop(&mut self) {
        self.terminate();
    }
}
impl ClassicAudioPreviewProducer {
    #[cfg(unix)]
    pub fn start(
        config: &ClassicAudioPreviewProcessConfig,
        session: &mut ClassicAudioSession<'_>,
        resources: &mut dyn ClassicAudioResources,
        handles: &[ClassicAudioResourceHandle],
    ) -> Result<Self, EditorError> {
        if config.preparation_deadline.is_zero()
            || config.preparation_deadline > Duration::from_secs(120)
            || config.max_pcm_bytes == 0
            || config.max_pcm_bytes > 512 * 1024 * 1024
            || config.max_encoded_bytes == 0
            || config.max_encoded_bytes > 32 * 1024 * 1024
            || config.max_rss_bytes == 0
        {
            return Err(error("INVALID_AUDIO_PLAN", "Invalid preview admission"));
        }
        resources.checkpoint()?;
        let plan = session.plan()?;
        if plan["identity"]["profile"] != "classic-realtime-v31-web-audio" {
            return Err(error("INVALID_AUDIO_PLAN", "A preview session is required"));
        }
        let mut process =
            BrowserProcess::start(&config.node, &config.runner, Duration::from_secs(3600))?;
        let input = process.input.take();
        let mut producer = Self {
            process: Some(process),
            input,
            execution: new_identity(),
            started: Instant::now(),
            next_id: 0,
            last_callback_id: 0,
            admitted: serde_json::Map::new(),
            bound_sources: None,
            resource_ids: ids(&plan["resources"], "assetId")?,
            voice_ids: ids(&plan["voices"], "elementId")?,
            fixed_voices: plan["voices"].clone(),
            runtime: None,
            prepared: None,
            identity: plan["identity"].clone(),
            max_pcm_bytes: config.max_pcm_bytes,
            max_rss_bytes: config.max_rss_bytes,
            strict: config.strict_cgroup.is_some(),
        };
        let result = (|| {
            producer.send(json!({"type":"begin","plan":plan,"resources":handles,
                "runtime":{"chromiumExecutable":config.chromium},"preparationDeadlineMs":config.preparation_deadline.as_millis() as u64,
                "admission":{"deadlineMs":3600000,"maxPcmBytes":config.max_pcm_bytes,"maxEncodedBytes":config.max_encoded_bytes,
                    "maxRssBytes":config.max_rss_bytes.to_string(),"quotaProfile":if producer.strict {"strict-hard"} else {"monitored-terminate"},
                    "cgroupDirectory":config.strict_cgroup}}))?;
            loop {
                if producer.started.elapsed() >= config.preparation_deadline {
                    return Err(error(
                        "AUDIO_TIME_LIMIT",
                        "Preview preparation exceeded its deadline",
                    ));
                }
                let Some(message) = producer.receive(resources)? else {
                    continue;
                };
                match string(&message, "type")? {
                    "started" => producer.register_browser(&message)?,
                    "ready" => producer.register_runtime(&message)?,
                    "request" => producer.callback(message, session, resources)?,
                    "prepared" => {
                        if producer.runtime.is_none() {
                            return Err(error(
                                "AUDIO_PROTOCOL_ERROR",
                                "Prepared sources preceded runtime readiness",
                            ));
                        }
                        let receipt = message.get("receipt").ok_or_else(|| {
                            error(
                                "AUDIO_PROTOCOL_ERROR",
                                "Missing preview preparation receipt",
                            )
                        })?;
                        let receipt = producer.expand_prepared_receipt(receipt)?;
                        producer.validate_receipt(&receipt)?;
                        let mut typed = receipt.clone();
                        typed
                            .as_object_mut()
                            .expect("Receipt validated")
                            .remove("sourceBindingSha256");
                        let typed: ClassicAudioPreviewReceipt = serde_json::from_value(typed)
                            .map_err(|_| {
                                error(
                                    "AUDIO_PROTOCOL_ERROR",
                                    "Invalid typed preview preparation receipt",
                                )
                            })?;
                        session.validate_preview_receipt(&typed)?;
                        producer.prepared = Some(receipt);
                        return Ok(());
                    }
                    _ => {
                        return Err(error(
                            "AUDIO_PROTOCOL_ERROR",
                            "Unexpected preview preparation packet",
                        ));
                    }
                }
            }
        })();
        if let Err(cause) = result {
            producer.terminate();
            return Err(cause);
        }
        Ok(producer)
    }

    pub fn ready_receipt(&self) -> &Value {
        self.prepared
            .as_ref()
            .expect("Prepared before construction completes")
    }
    pub fn runtime_identity(&self) -> &Value {
        self.runtime
            .as_ref()
            .expect("Ready before construction completes")
    }

    pub fn next(
        &mut self,
        session: &mut ClassicAudioSession<'_>,
        resources: &mut dyn ClassicAudioResources,
        generation: u64,
        overlay_epoch: u64,
        start_sample: u64,
        frame_count: u32,
    ) -> Result<ClassicAudioPreviewPcmBlock, EditorError> {
        if !(1..=4096).contains(&frame_count) {
            return Err(error(
                "INVALID_AUDIO_PLAN",
                "Preview block exceeds its credit",
            ));
        }
        self.next_id = self.next_id.checked_add(1).ok_or_else(|| {
            error(
                "AUDIO_BUDGET_EXCEEDED",
                "Preview command identity exhausted",
            )
        })?;
        let id = self.next_id.to_string();
        let fixed = json!({"commandId":id,"generation":generation.to_string(),"overlayEpoch":overlay_epoch.to_string(),"startSample":start_sample.to_string()});
        let result = (|| {
            self.send(json!({"type":"command","op":"next","id":id,"generation":generation.to_string(),
                "overlayEpoch":overlay_epoch.to_string(),"startSample":start_sample.to_string(),"frameCount":frame_count}))?;
            let mut channels: [Option<Vec<f32>>; 2] = [None, None];
            loop {
                let Some(message) = self.receive(resources)? else {
                    continue;
                };
                match string(&message, "type")? {
                    "request" => self.callback(message, session, resources)?,
                    "pcm" | "block" => {
                        if ["commandId", "generation", "overlayEpoch", "startSample"]
                            .iter()
                            .any(|k| message[k] != fixed[k])
                        {
                            return Err(error(
                                "AUDIO_PROTOCOL_ERROR",
                                "Preview block changed its captured command",
                            ));
                        }
                        if string(&message, "type")? == "pcm" {
                            let channel = number(&message, "channel")?;
                            if channel > 1
                                || channels[channel as usize].is_some()
                                || number(&message, "frames")? != u64::from(frame_count)
                            {
                                return Err(error(
                                    "AUDIO_PROTOCOL_ERROR",
                                    "Preview PCM repeats or exceeds its credit",
                                ));
                            }
                            channels[channel as usize] =
                                Some(decode_pcm(string(&message, "bytesHex")?, frame_count)?);
                        } else {
                            if number(&message, "frameCount")? != u64::from(frame_count)
                                || channels.iter().any(Option::is_none)
                            {
                                return Err(error(
                                    "AUDIO_PROTOCOL_ERROR",
                                    "Incomplete preview PCM block",
                                ));
                            }
                            let receipt = message.get("receipt").ok_or_else(|| {
                                error("AUDIO_PROTOCOL_ERROR", "Missing preview block receipt")
                            })?;
                            self.validate_completion(receipt)?;
                            if self.prepared.as_ref().is_none_or(|p| {
                                p["sourceBindingSha256"] != receipt["sourceBindingSha256"]
                            }) {
                                return Err(error(
                                    "AUDIO_PROTOCOL_ERROR",
                                    "Preview sources changed across commands",
                                ));
                            }
                            return Ok(ClassicAudioPreviewPcmBlock {
                                generation,
                                overlay_epoch,
                                start_sample,
                                frame_count,
                                channels: channels.map(|c| c.expect("Completeness checked")),
                                receipt: receipt.clone(),
                            });
                        }
                    }
                    _ => {
                        return Err(error(
                            "AUDIO_PROTOCOL_ERROR",
                            "Unexpected preview block packet",
                        ));
                    }
                }
            }
        })();
        if result.is_err() {
            self.terminate();
        }
        result
    }

    /// Called by host assembly's idle authorization timer, only between
    /// commands. Also observes a realm/quota failure without reaping a PID
    /// independently of the shared process supervisor.
    pub fn checkpoint(
        &mut self,
        resources: &mut dyn ClassicAudioResources,
    ) -> Result<(), EditorError> {
        // Check queued failure/identity facts without delaying a ready command.
        let result = self
            .receive_timeout(resources, Duration::ZERO)
            .and_then(|message| {
                if message.is_some() {
                    return Err(error(
                        "AUDIO_PROTOCOL_ERROR",
                        "Unexpected packet from idle preview producer",
                    ));
                }
                Ok(())
            });
        if result.is_err() {
            self.terminate();
        }
        result
    }
    fn check_authorized(
        &mut self,
        resources: &mut dyn ClassicAudioResources,
    ) -> Result<(), EditorError> {
        let result = resources.checkpoint().and_then(|()| {
            if self.process.is_none() {
                return Err(error(
                    "AUDIO_SESSION_CLOSED",
                    "Preview producer has stopped",
                ));
            }
            if self.started.elapsed() >= Duration::from_secs(3600) {
                return Err(error("AUDIO_TIME_LIMIT", "Preview session TTL expired"));
            }
            Ok(())
        });
        if result.is_err() {
            self.terminate();
        }
        result
    }

    pub fn stop(&mut self, resources: &mut dyn ClassicAudioResources) -> Result<(), EditorError> {
        if self.process.is_none() {
            return Ok(());
        }
        let result = (|| {
            self.send(json!({"type":"command","op":"stop"}))?;
            let since = Instant::now();
            loop {
                if since.elapsed() > Duration::from_secs(15) {
                    return Err(error(
                        "AUDIO_REALM_TERMINATION_FAILED",
                        "Preview stop was not acknowledged",
                    ));
                }
                let Some(message) = self.receive(resources)? else {
                    continue;
                };
                if string(&message, "type")? != "stopped" {
                    return Err(error(
                        "AUDIO_PROTOCOL_ERROR",
                        "Unexpected preview stop packet",
                    ));
                }
                if self.runtime.as_ref() != message.get("identity") {
                    return Err(error(
                        "AUDIO_ENGINE_CHANGED",
                        "Preview stop changed its engine",
                    ));
                }
                let quota: AudioQuotaReceipt = serde_json::from_value(message["quota"].clone())
                    .map_err(|_| error("AUDIO_PROTOCOL_ERROR", "Invalid preview quota receipt"))?;
                if quota.profile
                    != if self.strict {
                        "strict-hard"
                    } else {
                        "monitored-terminate"
                    }
                    || decimal(&quota.max_rss_bytes)? != self.max_rss_bytes
                    || decimal(&quota.observed_peak_rss_bytes)? > self.max_rss_bytes
                    || quota.hard_memory_enforced != self.strict
                    || !(20..=250).contains(&quota.monitor_interval_ms)
                    || quota.elapsed_ms > 3600000
                {
                    return Err(error(
                        "AUDIO_PROTOCOL_ERROR",
                        "Preview quota receipt changed admission",
                    ));
                }
                return Ok(());
            }
        })();
        self.input.take();
        let cleanup = self
            .process
            .take()
            .map(|p| p.finish(result.is_ok(), &self.execution))
            .unwrap_or(Ok(()));
        result.and(cleanup)
    }

    fn send(&mut self, message: Value) -> Result<(), EditorError> {
        packet(
            self.input
                .as_mut()
                .ok_or_else(|| error("AUDIO_SESSION_CLOSED", "Preview input is closed"))?,
            &self.execution,
            message,
        )
    }
    fn receive(
        &mut self,
        resources: &mut dyn ClassicAudioResources,
    ) -> Result<Option<Value>, EditorError> {
        self.receive_timeout(resources, Duration::from_millis(50))
    }
    fn receive_timeout(
        &mut self,
        resources: &mut dyn ClassicAudioResources,
        timeout: Duration,
    ) -> Result<Option<Value>, EditorError> {
        self.check_authorized(resources)?;
        let message = self
            .process
            .as_mut()
            .expect("Checkpoint verified process")
            .receive_timeout(timeout)?;
        if let Some(message) = &message {
            if number(message, "protocolVersion")? != 1
                || string(message, "executionId")? != self.execution
            {
                return Err(error(
                    "AUDIO_PROTOCOL_ERROR",
                    "Preview execution identity changed",
                ));
            }
            if matches!(string(message, "type")?, "failed" | "cancelled") {
                return Err(error(
                    message
                        .pointer("/error/code")
                        .and_then(Value::as_str)
                        .unwrap_or("AUDIO_HOST_FAILED"),
                    "Isolated preview execution failed",
                ));
            }
        }
        Ok(message)
    }
    fn callback(
        &mut self,
        mut message: Value,
        session: &mut ClassicAudioSession<'_>,
        resources: &mut dyn ClassicAudioResources,
    ) -> Result<(), EditorError> {
        if self.runtime.is_none() {
            return Err(error(
                "AUDIO_PROTOCOL_ERROR",
                "Preview callback preceded readiness",
            ));
        }
        let id = string(&message, "id")?.to_owned();
        let sequence = decimal(&id)?;
        if sequence != self.last_callback_id + 1 || sequence > 100000 {
            return Err(error(
                "AUDIO_PROTOCOL_ERROR",
                "Invalid preview callback identity",
            ));
        }
        self.last_callback_id = sequence;
        let packed_version = match string(&message, "op")? {
            "compilePreviewBlockPackedV1" => 1,
            "compilePreviewBlockPackedV2" => 2,
            _ => 0,
        };
        let packed_block = packed_version != 0;
        // Compression belongs to this private transport adapter. Application
        // receives and validates the unchanged, complete typed PCM facts.
        if string(&message, "op")? == "bindPreviewSourcesSharedV1" {
            let formats = expand_source_binding(&message["args"], &self.fixed_voices)?;
            message["op"] = json!("bindPreviewSources");
            message["args"] = json!({"sourceFormats": formats});
        }
        if packed_block {
            message["op"] = json!("compilePreviewBlock");
        }
        let callback: AudioCallback =
            serde_json::from_value(json!({"op":message.get("op"),"args":message.get("args")}))
                .map_err(|_| error("AUDIO_PROTOCOL_ERROR", "Invalid typed preview callback"))?;
        if packed_block {
            let AudioCallback::PreviewBlock(request) = callback else {
                return Err(error(
                    "AUDIO_PROTOCOL_ERROR",
                    "Invalid typed preview callback",
                ));
            };
            let block = session.compile_preview_block_typed(request, resources)?;
            let bytes = if packed_version == 2 {
                // V2 carries native JSON text through Node without parsing and
                // re-stringifying sample facts (which normalizes negative zero).
                let text = String::from_utf8(pack_preview_block_v2(block)?)
                    .map_err(|_| error("AUDIO_PROTOCOL_ERROR", "Invalid packed preview UTF-8"))?;
                serde_json::to_vec(&text).map_err(|_| {
                    error(
                        "AUDIO_PROTOCOL_ERROR",
                        "Could not encode packed preview text",
                    )
                })?
            } else {
                pack_preview_block(block)?
            };
            return reply_encoded(
                self.input
                    .as_mut()
                    .ok_or_else(|| error("AUDIO_SESSION_CLOSED", "Preview input closed"))?,
                &self.execution,
                &id,
                &bytes,
            );
        }
        let value = session.callback(callback, resources)?;
        // Capture only facts which the authoritative session has accepted.
        // Source/primary receipts must cover those exact bindings later.
        match string(&message, "op")? {
            "admitDecode" => {
                let asset = string(&message["args"]["resource"], "assetId")?;
                if !self.resource_ids.iter().any(|known| known == asset)
                    || self
                        .admitted
                        .get(asset)
                        .is_some_and(|prior| prior["primary"] != message["args"]["primary"])
                {
                    return Err(error(
                        "AUDIO_PROTOCOL_ERROR",
                        "Preview primary admission changed",
                    ));
                }
                self.admitted.insert(
                    asset.into(),
                    json!({"primary":message["args"]["primary"],"admission":value}),
                );
            }
            "bindPreviewSources" => {
                let formats = &message["args"]["sourceFormats"];
                if self.bound_sources.is_some() || ids(formats, "elementId")? != self.voice_ids {
                    return Err(error(
                        "AUDIO_PROTOCOL_ERROR",
                        "Preview source binding coverage changed",
                    ));
                }
                self.bound_sources = Some(formats.clone());
            }
            _ => {}
        }
        reply(
            self.input
                .as_mut()
                .ok_or_else(|| error("AUDIO_SESSION_CLOSED", "Preview input closed"))?,
            &self.execution,
            &id,
            value,
        )
    }
    fn terminate(&mut self) {
        if let Some(input) = self.input.as_mut() {
            let _ = packet(input, &self.execution, json!({"type":"cancel"}));
        }
        self.input.take();
        if let Some(process) = self.process.take() {
            let _ = process.finish(false, &self.execution);
        }
    }
}
