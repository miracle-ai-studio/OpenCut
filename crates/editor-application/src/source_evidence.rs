//! Builds a portable contact sheet from verified, authorized source observations.
use opencut_contracts::{EditorError, source_art::*, source_evidence::*};
use sha2::{Digest, Sha256};
pub trait SourceEvidencePort {
    fn frame(&mut self, input: SourceFrameRequest) -> Result<SourceFrameResponse, EditorError>;
    fn waveform(&mut self, input: SourceArtRequest) -> Result<SourceArtResponse, EditorError>;
}
fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = *chunk.get(1).unwrap_or(&0);
        let c = *chunk.get(2).unwrap_or(&0);
        out.push(TABLE[(a >> 2) as usize] as char);
        out.push(TABLE[(((a & 3) << 4) | (b >> 4)) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(((b & 15) << 2) | (c >> 6)) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(c & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}
fn unhex(s: &str) -> Result<Vec<u8>, EditorError> {
    if s.len() % 2 != 0 || s.len() > 2 * 1024 * 1024 {
        return Err(invalid("Invalid source JPEG byte count"));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(
                s.get(i..i + 2).ok_or_else(|| invalid("Invalid JPEG hex"))?,
                16,
            )
            .map_err(|_| invalid("Invalid JPEG hex"))
        })
        .collect()
}
pub fn create(
    port: &mut impl SourceEvidencePort,
    input: SourceEvidenceRequest,
) -> Result<SourceEvidenceResult, EditorError> {
    if input.request_id.is_empty()
        || input.request_id.len() > 900
        || input.revision.is_empty()
        || input.frames.len() > 12
        || input.waveforms.len() > 8
        || input.frames.is_empty() && input.waveforms.is_empty()
    {
        return Err(invalid(
            "Evidence needs a fixed revision, bounded request ID and 1..12 frames or 1..8 waveforms",
        ));
    }
    let rows = input.frames.len().div_ceil(3);
    let height = rows * 208 + input.waveforms.len() * 120;
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"960\" height=\"{height}\" viewBox=\"0 0 960 {height}\"><rect width=\"960\" height=\"{height}\" fill=\"#111\"/>"
    );
    let mut receipts = vec![];
    for (index, point) in input.frames.iter().enumerate() {
        let result = port.frame(SourceFrameRequest {
            project_id: input.project_id.clone(),
            revision: input.revision.clone(),
            asset_id: point.asset_id.clone(),
            request_id: format!("{}:frame:{index}", input.request_id),
            time: point.time.clone(),
        })?;
        if result.project_id != input.project_id
            || result.revision != input.revision
            || result.asset_id != point.asset_id
            || result.requested_time != point.time
        {
            return Err(invalid("Source frame scope differs"));
        }
        let SourceArtData::ThumbnailReady {
            content_hash,
            jpeg_bytes_hex,
            observation,
            ..
        } = result.data
        else {
            return Err(EditorError::new(
                "SOURCE_FRAME_NOT_FOUND",
                "No image at the requested source point",
            ));
        };
        let bytes = unhex(&jpeg_bytes_hex)?;
        if format!("{:x}", Sha256::digest(&bytes)) != content_hash {
            return Err(invalid("Source JPEG hash differs"));
        }
        let x = index % 3 * 320;
        let y = index / 3 * 208;
        let label = escape(&format!(
            "{} @ {}/{}",
            point.asset_id, point.time.ticks, point.time.timebase
        ));
        svg.push_str(&format!("<image x=\"{x}\" y=\"{y}\" width=\"320\" height=\"180\" preserveAspectRatio=\"xMidYMid meet\" href=\"data:image/jpeg;base64,{}\"/><text x=\"{}\" y=\"{}\" fill=\"white\" font-family=\"sans-serif\" font-size=\"12\">{label}</text>",base64(&bytes),x+8,y+198));
        receipts.push(SourceEvidenceReceipt {
            asset_id: point.asset_id.clone(),
            version: result.version,
            identity: result.identity,
            requested_time: Some(point.time.clone()),
            requested_range: None,
            frame_observation: Some(observation),
        });
        if svg.len() > 800_000 {
            return Err(EditorError::new(
                "RESOURCE_TOO_LARGE",
                "Reduce contact-sheet frame count",
            ));
        }
    }
    for (index, wave) in input.waveforms.iter().enumerate() {
        let result = port.waveform(SourceArtRequest {
            project_id: input.project_id.clone(),
            revision: input.revision.clone(),
            asset_id: wave.asset_id.clone(),
            request_id: format!("{}:wave:{index}", input.request_id),
            query: SourceArtQuery::Waveform {
                sample_rate: wave.sample_rate,
                projection: WaveformProjection::Source {
                    range: Some(wave.range.clone()),
                    bins: wave.bins,
                },
            },
        })?;
        if result.project_id != input.project_id
            || result.revision != input.revision
            || result.asset_id != wave.asset_id
        {
            return Err(invalid("Source waveform scope differs"));
        }
        let SourceArtData::Waveform { amplitudes, .. } = result.data else {
            return Err(invalid("Expected source waveform"));
        };
        let y = rows * 208 + index * 120;
        let mut path = String::new();
        let count = amplitudes.len().max(1);
        for (i, peak) in amplitudes.into_iter().enumerate() {
            if !peak.is_finite() || peak < 0.0 {
                return Err(invalid("Invalid source peak"));
            }
            let x = 8.0 + 944.0 * i as f64 / count as f64;
            let half = f64::from(peak).min(1.0) * 40.0;
            path.push_str(&format!(
                "M{x:.3} {:.3}v{:.3}",
                y as f64 + 48.0 - half,
                2.0 * half
            ));
        }
        svg.push_str(&format!("<path d=\"{path}\" stroke=\"#61b9f9\"/><text x=\"8\" y=\"{}\" fill=\"white\" font-size=\"12\">{} · source peaks</text>",y+110,escape(&wave.asset_id)));
        receipts.push(SourceEvidenceReceipt {
            asset_id: wave.asset_id.clone(),
            version: result.version,
            identity: result.identity,
            requested_time: None,
            requested_range: Some(wave.range.clone()),
            frame_observation: None,
        });
    }
    svg.push_str("</svg>");
    let bytes = svg.as_bytes();
    if bytes.len() > 900_000 {
        return Err(EditorError::new(
            "RESOURCE_TOO_LARGE",
            "Reduce evidence pack size",
        ));
    }
    Ok(SourceEvidenceResult {
        project_id: input.project_id,
        revision: input.revision,
        receipts,
        contact_sheet: InlineEvidenceArtifact {
            mime_type: "image/svg+xml".into(),
            content_hash: format!("{:x}", Sha256::digest(bytes)),
            bytes_hex: bytes.iter().map(|byte| format!("{byte:02x}")).collect(),
        },
        waveform_profile: "source-peaks128-v1; no gain, mastering or PCM claim".into(),
    })
}
