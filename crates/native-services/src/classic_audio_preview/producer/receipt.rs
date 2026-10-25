//! Actual runtime and accepted source-binding receipt fences.
use super::*;
use sha2::{Digest, Sha256};

impl ClassicAudioPreviewProducer {
    pub(super) fn expand_prepared_receipt(&self, receipt: &Value) -> Result<Value, EditorError> {
        let mut receipt = receipt.clone();
        let fields = receipt.as_object_mut().ok_or_else(|| {
            error(
                "AUDIO_PROTOCOL_ERROR",
                "Invalid preview preparation receipt",
            )
        })?;
        if let Some(binding) = fields.remove("sourceBinding") {
            if fields.contains_key("sourceFormats") {
                return Err(error(
                    "AUDIO_PROTOCOL_ERROR",
                    "Ambiguous preview source binding",
                ));
            }
            fields.insert(
                "sourceFormats".into(),
                expand_source_binding(&binding, &self.fixed_voices)?,
            );
        }
        Ok(receipt)
    }
    pub(super) fn register_browser(&mut self, message: &Value) -> Result<(), EditorError> {
        let process = self.process.as_ref().expect("Active preparation");
        let pid = number(message, "browserPid")?;
        if pid == 0
            || pid > i32::MAX as u64
            || pid == u64::from(process.guard.child.id())
            || process.guard.browser_pid.load(Ordering::Acquire) != 0
        {
            return Err(error(
                "AUDIO_PROTOCOL_ERROR",
                "Invalid preview browser identity",
            ));
        }
        process
            .guard
            .browser_pid
            .store(pid as u32, Ordering::Release);
        Ok(())
    }
    pub(super) fn register_runtime(&mut self, message: &Value) -> Result<(), EditorError> {
        if self.runtime.is_some()
            || number(message, "browserPid")? == 0
            || number(message, "browserPid")?
                != u64::from(
                    self.process
                        .as_ref()
                        .expect("Active preparation")
                        .guard
                        .browser_pid
                        .load(Ordering::Acquire),
                )
        {
            return Err(error(
                "AUDIO_PROTOCOL_ERROR",
                "Preview readiness changed browser PID",
            ));
        }
        let identity: ClassicAudioEngineIdentity =
            serde_json::from_value(message["identity"].clone())
                .map_err(|_| error("AUDIO_ENGINE_CHANGED", "Invalid preview runtime identity"))?;
        if identity.playwright_version != "1.63.0"
            || identity.mediabunny_version != "1.41.0"
            || identity.engine.is_empty()
            || [&identity.browser_sha256, &identity.module_sha256]
                .iter()
                .any(|hash| hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err(error(
                "AUDIO_ENGINE_CHANGED",
                "Preview runtime is not pinned",
            ));
        }
        self.runtime = Some(message["identity"].clone());
        Ok(())
    }
    pub(super) fn validate_receipt(&self, receipt: &Value) -> Result<(), EditorError> {
        self.validate_completion(receipt)?;
        if !receipt["sourceFormats"].is_array() || !receipt["decoders"].is_array() {
            return Err(error(
                "AUDIO_PROTOCOL_ERROR",
                "Missing prepared source facts",
            ));
        }
        let binding = json!({"decoders":receipt["decoders"],"sourceFormats":receipt["sourceFormats"],"preparation":receipt["preparation"]});
        let serialized = serde_json::to_vec(&canonical_binding(&binding)?)
            .map_err(|_| error("AUDIO_PROTOCOL_ERROR", "Invalid preview binding facts"))?;
        if receipt["sourceBindingSha256"] != format!("{:x}", Sha256::digest(serialized)) {
            return Err(error(
                "AUDIO_PROTOCOL_ERROR",
                "Preview binding digest differs from accepted facts",
            ));
        }
        self.validate_sources(receipt)
    }
    pub(super) fn validate_completion(&self, receipt: &Value) -> Result<(), EditorError> {
        if receipt["identity"] != self.identity
            || receipt["engineIdentity"]
                != self.runtime.as_ref().expect("Runtime checked")["engine"]
            || number(receipt, "peakAccountedBytes")? > self.max_pcm_bytes
            || receipt["sourceBindingSha256"].as_str().is_none_or(|h| {
                h.len() != 64
                    || !h
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            })
        {
            return Err(error(
                "AUDIO_PROTOCOL_ERROR",
                "Preview receipt changed its fixed execution",
            ));
        }
        Ok(())
    }
    pub(super) fn validate_current_binding(&self, receipt: &Value) -> Result<(), EditorError> {
        let prepared = self
            .prepared
            .as_ref()
            .ok_or_else(|| error("AUDIO_PROTOCOL_ERROR", "Missing initial source identity"))?;
        let binding = json!({"decoders":prepared["decoders"],"sourceFormats":self.bound_sources,
            "preparation":prepared["preparation"],"window":self.active_window});
        let bytes = serde_json::to_vec(&canonical_binding(&binding)?)
            .map_err(|_| error("AUDIO_PROTOCOL_ERROR", "Invalid current source binding"))?;
        if receipt["sourceBindingSha256"] != format!("{:x}", Sha256::digest(bytes)) {
            return Err(error(
                "AUDIO_PROTOCOL_ERROR",
                "PCM completion differs from the accepted source window",
            ));
        }
        Ok(())
    }
    pub(super) fn validate_sources(&self, receipt: &Value) -> Result<(), EditorError> {
        if self.bound_sources.as_ref() != Some(&receipt["sourceFormats"])
            || ids(&receipt["decoders"], "assetId")? != self.resource_ids
            || self.admitted.len() != self.resource_ids.len()
        {
            return Err(error(
                "AUDIO_PROTOCOL_ERROR",
                "Preview receipt omitted accepted source facts",
            ));
        }
        for decoder in receipt["decoders"].as_array().expect("Array checked") {
            let asset = string(decoder, "assetId")?;
            let admitted = self
                .admitted
                .get(asset)
                .ok_or_else(|| error("AUDIO_PROTOCOL_ERROR", "Preview decoder was not admitted"))?;
            // Rolling primary probes have no fabricated complete-source
            // extent. Prepared sources retain their observed native bound.
            let allocation = &admitted["allocation"];
            let known_extent = !allocation["preparation"].is_null();
            let max_frames = if known_extent {
                if allocation["preparation"]["mode"] == "windowed" {
                    decimal(string(&allocation["preparation"], "maxNativeFrames")?)?
                } else {
                    number(&allocation["preparation"], "maxFrames")?
                }
            } else {
                if allocation["stream"].is_null() {
                    return Err(error(
                        "AUDIO_PROTOCOL_ERROR",
                        "Preview source has no admitted consumer",
                    ));
                }
                number(&allocation["stream"], "workingFrames")?
            };
            if decoder["primary"] != admitted["primary"]
                || number(decoder, "sampleRate")? != number(&admitted["primary"], "sampleRate")?
                || number(decoder, "channels")? != number(&admitted["primary"], "channels")?.min(2)
                || if !known_extent {
                    !decoder["frameCount"].is_null()
                } else {
                    decimal(string(decoder, "frameCount")?)? > max_frames
                }
            {
                return Err(error(
                    "AUDIO_PROTOCOL_ERROR",
                    "Preview decoder receipt changed primary or admission",
                ));
            }
        }
        Ok(())
    }
}
