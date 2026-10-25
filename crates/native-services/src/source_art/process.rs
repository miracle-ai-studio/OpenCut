use super::*;
use crate::{
    browser_process::BrowserProcess, browser_resources::VerifiedBrowserResources,
    classic_audio::transport,
};
use opencut_composition::scene::CasResource;
use opencut_contracts::AssetVersion;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{sync::atomic::Ordering, time::Instant};
fn text<'a>(v: &'a Value, k: &str) -> Result<&'a str, EditorError> {
    v.get(k)
        .and_then(Value::as_str)
        .ok_or_else(|| protocol("Missing source-art text field"))
}
fn number(v: &Value, k: &str) -> Result<u64, EditorError> {
    v.get(k)
        .and_then(Value::as_u64)
        .ok_or_else(|| protocol("Missing source-art integer field"))
}
fn decimal(v: &str) -> Result<u64, EditorError> {
    let n = v
        .parse::<u64>()
        .map_err(|_| protocol("Invalid source-art decimal"))?;
    if n.to_string() != v {
        return Err(protocol("Noncanonical source-art decimal"));
    }
    Ok(n)
}
fn hex(v: &str) -> Result<Vec<u8>, EditorError> {
    if v.is_empty()
        || v.len() > 131072
        || !v.len().is_multiple_of(2)
        || !v
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err(protocol("Invalid bounded source-art byte chunk"));
    }
    transport::decode_hex(v)
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Receipt {
    Waveform {
        identity: SourceArtIdentity,
        sample_rate: u32,
        channels: u32,
        total_samples: String,
        bucket_size: u32,
        output_bytes: usize,
    },
    ThumbnailReady {
        identity: SourceArtIdentity,
        width: u32,
        height: u32,
        observation: SourceArtThumbnailObservation,
        output_bytes: usize,
    },
    ThumbnailNoFrame {
        identity: SourceArtIdentity,
        observation: SourceArtThumbnailObservation,
        reason: SourceArtNoFrameReason,
        output_bytes: usize,
    },
}
fn decoded(
    receipt: Receipt,
    bytes: Vec<u8>,
    version: AssetVersion,
) -> Result<DecodedSourceArt, EditorError> {
    Ok(match receipt {
        Receipt::Waveform {
            identity,
            sample_rate,
            channels,
            total_samples,
            bucket_size,
            output_bytes,
        } => {
            if output_bytes != bytes.len() || !bytes.len().is_multiple_of(4) {
                return Err(protocol("Source peaks completion size differs"));
            }
            DecodedSourceArt::Waveform {
                version,
                identity,
                sample_rate,
                channels,
                total_samples: decimal(&total_samples)?,
                bucket_size,
                amplitudes: bytes
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                    .collect(),
            }
        }
        Receipt::ThumbnailReady {
            identity,
            width,
            height,
            observation,
            output_bytes,
        } => {
            if output_bytes != bytes.len() {
                return Err(protocol("JPEG completion size differs"));
            }
            DecodedSourceArt::ThumbnailReady {
                version,
                identity,
                width,
                height,
                jpeg_bytes: bytes,
                observation,
            }
        }
        Receipt::ThumbnailNoFrame {
            identity,
            observation,
            reason,
            output_bytes,
        } => {
            if output_bytes != 0 || !bytes.is_empty() {
                return Err(protocol("No-frame result contains bytes"));
            }
            DecodedSourceArt::ThumbnailNoFrame {
                version,
                identity,
                observation,
                reason,
            }
        }
    })
}
#[cfg(unix)]
pub(super) fn execute(
    executor: &SourceArtProcessExecutor,
    prepared: &PreparedSourceArt,
    checkpoint: &mut dyn SourceArtCheckpoint,
) -> Result<DecodedSourceArt, EditorError> {
    let config = &executor.config;
    let policy = prepared.policy();
    if config.deadline != Duration::from_millis(policy.deadline_ms)
        || config.max_encoded_bytes != policy.max_encoded_bytes
        || config.max_pcm_bytes != policy.max_pcm_bytes
        || config.max_rss_bytes != policy.max_rss_bytes
    {
        return Err(protocol("Source-art captured admission differs"));
    }
    source_art_decode_key(prepared, &config.identity)?;
    let asset = prepared.asset();
    let resource = CasResource {
        asset_id: asset.id.clone(),
        version: asset.version.clone(),
        kind: asset.kind.clone(),
        duration: asset.duration.clone(),
        metadata: asset.metadata.clone(),
        license: asset.license.clone(),
    };
    let mut resources = VerifiedBrowserResources::new(
        &executor.asset_root,
        [&resource],
        || checkpoint.checkpoint(),
        "SOURCE_ART_PROTOCOL_ERROR",
    )?;
    let handles: Vec<_> = resources
        .handles()
        .map(|(asset_id, handle)| json!({"assetId":asset_id,"handle":handle}))
        .collect();
    let query = match prepared.request().query {
        SourceArtQuery::Waveform { sample_rate, .. } => {
            json!({"kind":"waveform","sampleRate":sample_rate})
        }
        SourceArtQuery::Thumbnail {} => {
            if let Some(at) = prepared.thumbnail_time() {
                let at = opencut_time::RationalTime::try_from(at)?;
                json!({"kind":"source_frame","seconds":at.ticks() as f64/f64::from(at.timebase())})
            } else {
                json!({"kind":"thumbnail"})
            }
        }
    };
    let kind = if matches!(prepared.request().query, SourceArtQuery::Waveform { .. }) {
        "waveform"
    } else {
        "thumbnail"
    };
    let limit = if kind == "waveform" {
        prepared.waveform_output_bytes()?
    } else {
        SOURCE_ART_MAX_JPEG_BYTES as u64
    };
    let waveform_working_bytes = if kind == "waveform" {
        Some(prepared.waveform_working_bytes()?)
    } else {
        None
    };
    let max_waveform_bytes = (kind == "waveform").then_some(limit);
    let execution = uuid::Uuid::new_v4().to_string();
    let started = Instant::now();
    let mut process = BrowserProcess::start(&config.node, &config.runner, config.deadline)?;
    let mut input = process
        .input
        .take()
        .ok_or_else(|| protocol("Missing source-art runner input"))?;
    let result = (|| {
        transport::packet(
            &mut input,
            &execution,
            json!({"type":"begin","plan":{"resource":{"assetId":asset.id,"contentHash":asset.version.content_hash,"size":asset.version.size,"kind":asset.kind},"query":query,"identity":config.identity,"rangeCacheBytes":prepared.range_cache_bytes()?,"waveformWorkingBytes":waveform_working_bytes,"maxWaveformBytes":max_waveform_bytes},"resources":handles,"runtime":{"chromiumExecutable":config.chromium},"admission":{"quotaProfile":"monitored-terminate","maxEncodedBytes":policy.max_encoded_bytes,"maxPcmBytes":policy.max_pcm_bytes,"maxRssBytes":policy.max_rss_bytes.to_string(),"deadlineMs":policy.deadline_ms}}),
        )?;
        let mut ready = false;
        let mut bytes = Vec::new();
        let mut last_request = 0u64;
        loop {
            resources.checkpoint()?;
            if started.elapsed() >= config.deadline {
                return Err(error(
                    "SOURCE_ART_TIME_LIMIT",
                    "Source-art decode exceeded deadline",
                ));
            }
            let Some(m) = process.receive()? else {
                continue;
            };
            if text(&m, "executionId")? != execution {
                return Err(protocol("Source-art execution envelope differs"));
            }
            match text(&m, "type")? {
                "started" => {
                    let pid = number(&m, "browserPid")?;
                    if pid == 0
                        || pid > i32::MAX as u64
                        || pid == u64::from(process.guard.child.id())
                        || process.guard.browser_pid.load(Ordering::Acquire) != 0
                    {
                        return Err(protocol("Invalid source-art browser PID"));
                    }
                    process
                        .guard
                        .browser_pid
                        .store(pid as u32, Ordering::Release);
                }
                "ready" => {
                    if ready
                        || number(&m, "browserPid")?
                            != u64::from(process.guard.browser_pid.load(Ordering::Acquire))
                        || process.guard.browser_pid.load(Ordering::Acquire) == 0
                    {
                        return Err(protocol("Invalid source-art readiness"));
                    }
                    let observed: SourceArtIdentity =
                        serde_json::from_value(m["identity"]["sourceArtIdentity"].clone())
                            .map_err(|_| protocol("Invalid source-art actual identity"))?;
                    if observed != config.identity {
                        return Err(error(
                            "ARTIFACT_VERIFICATION_FAILED",
                            "Source-art actual runtime differs from captured identity",
                        ));
                    }
                    ready = true;
                }
                "request" => {
                    if !ready {
                        return Err(protocol("Source-art callback before ready"));
                    }
                    let id = text(&m, "id")?;
                    let sequence = decimal(id)?;
                    if sequence != last_request + 1 || sequence > 9_007_199_254_740_991 {
                        return Err(protocol("Invalid source-art callback identity"));
                    }
                    last_request = sequence;
                    let args = &m["args"];
                    let value = match text(&m, "op")? {
                        "resolve" => {
                            let r = &args["resource"];
                            if text(r, "assetId")? != asset.id
                                || text(r, "kind")?
                                    != serde_json::to_value(&asset.kind)
                                        .map_err(|_| protocol("Invalid asset kind"))?
                                        .as_str()
                                        .unwrap_or("")
                            {
                                return Err(protocol("Fixed source-art resource differs"));
                            }
                            let (id, size) = resources.resolve(
                                text(r, "assetId")?,
                                text(r, "contentHash")?,
                                number(r, "size")?,
                                text(args, "handle")?,
                            )?;
                            json!({"id":id,"size":size,"content_hash":asset.version.content_hash,"max_chunk_bytes":65536})
                        }
                        "read" => {
                            let offset = decimal(text(args, "offset")?)?;
                            let count = u32::try_from(number(args, "count")?)
                                .map_err(|_| protocol("Invalid source read count"))?;
                            let (b, eof) =
                                resources.read(text(args, "transfer_id")?, offset, count)?;
                            let encoded: String = b.iter().map(|b| format!("{b:02x}")).collect();
                            json!({"bytes_hex":encoded,"eof":eof,"offset":offset.to_string()})
                        }
                        "release" => {
                            resources.release(text(args, "transfer_id")?)?;
                            json!({})
                        }
                        _ => return Err(protocol("Unknown source-art resource operation")),
                    };
                    transport::reply(&mut input, &execution, id, value)?;
                }
                "art_bytes" => {
                    if !ready
                        || text(&m, "kind")? != kind
                        || decimal(text(&m, "offset")?)? != bytes.len() as u64
                    {
                        return Err(protocol("Source-art output order differs"));
                    }
                    let chunk = hex(text(&m, "bytes_hex")?)?;
                    if bytes.len() as u64 + chunk.len() as u64 > limit {
                        return Err(error(
                            "SOURCE_ART_BUDGET_EXCEEDED",
                            "Source-art output exceeds byte budget",
                        ));
                    }
                    bytes.extend(chunk);
                }
                "complete" => {
                    if !ready {
                        return Err(protocol("Source-art completion before ready"));
                    }
                    validate_quota(&m["quota"], policy)?;
                    let r: Receipt = serde_json::from_value(m["receipt"].clone())
                        .map_err(|_| protocol("Invalid typed source-art receipt"))?;
                    let output = decoded(r, bytes, asset.version.clone())?;
                    if output.identity() != &config.identity {
                        return Err(protocol("Source-art receipt identity changed"));
                    }
                    validate_decoded_source_art(prepared, &output)?;
                    return Ok(output);
                }
                "failed" => {
                    let code = m["error"]["code"]
                        .as_str()
                        .filter(|c| {
                            !c.is_empty()
                                && c.len() <= 64
                                && c.bytes().all(|b| {
                                    b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_'
                                })
                        })
                        .unwrap_or("SOURCE_ART_DECODE_FAILED");
                    return Err(error(code, "Isolated source-art decoding failed"));
                }
                "cancelled" => return Err(error("CANCELLED", "Source-art decode cancelled")),
                _ => return Err(protocol("Unknown source-art packet")),
            }
        }
    })();
    if result.is_err() {
        let _ = transport::packet(&mut input, &execution, json!({"type":"cancel"}));
    }
    drop(input);
    // Real process exit is acknowledged while the verified CAS read gate is held.
    process.finish(result.is_ok(), &execution)?;
    let output = result?;
    resources.checkpoint()?;
    Ok(output)
}
fn validate_quota(v: &Value, p: &SourceArtPolicy) -> Result<(), EditorError> {
    if text(v, "profile")? != "monitored-terminate"
        || decimal(text(v, "maxRssBytes")?)? != p.max_rss_bytes
        || decimal(text(v, "observedPeakRssBytes")?)? > p.max_rss_bytes
        || v["hardMemoryEnforced"] != false
        || !(20..=250).contains(&number(v, "monitorIntervalMs")?)
        || number(v, "elapsedMs")? > p.deadline_ms
    {
        return Err(protocol("Source-art quota receipt differs"));
    }
    Ok(())
}
#[cfg(test)]
pub(super) fn decode_for_test(
    value: Value,
    bytes: Vec<u8>,
) -> Result<DecodedSourceArt, EditorError> {
    decoded(
        serde_json::from_value(value).map_err(|_| protocol("Invalid receipt"))?,
        bytes,
        AssetVersion {
            id: format!("av_{}", "d".repeat(64)),
            content_hash: "d".repeat(64),
            size: 1,
        },
    )
}
