//! Read-only source-art preparation and projection. Native adapters decode the
//! source; this module owns fixed selection, receipts and exact display clocks.
use opencut_contracts::{AssetKind, EditorError, project_v4::*, source_art_v1::*};
use opencut_job_execution::project_v4_audio::TrustedJobAuthorityV1;
use opencut_time::{RationalTime, validate_range};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;

mod ports;
mod projection;
mod validation;
pub use ports::*;
pub use validation::{validate_cancel, validate_policy, validate_request};
#[cfg(test)]
mod tests;

pub const SOURCE_PEAKS_ALGORITHM_V1: &str = "source-peaks128-v1";
pub const THUMBNAIL_ALGORITHM_V1: &str = "classic-thumbnail-1s-jpeg08-v1";
pub const SOURCE_ART_BUCKET_SIZE_V1: u32 = 128;
pub const SOURCE_ART_MAX_JPEG_BYTES_V1: usize = 1024 * 1024;

fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
fn receipt(message: &str) -> EditorError {
    EditorError::new("ARTIFACT_VERIFICATION_FAILED", message)
}
fn budget(message: &str) -> EditorError {
    EditorError::new("SOURCE_ART_BUDGET_EXCEEDED", message)
}
fn overflow() -> EditorError {
    EditorError::new("TIME_OVERFLOW", "Source-art sample clock overflow")
}

#[derive(Clone, Debug)]
pub struct PreparedSourceArtV1 {
    request: SourceArtRequestV1,
    asset: AssetV4,
    authority: TrustedJobAuthorityV1,
    scope: String,
    policy: SourceArtPolicyV1,
    element_clock: Option<ElementClock>,
}

#[derive(Clone, Debug)]
struct ElementClock {
    source_start: RationalTime,
    rate: RationalRateV4,
}

impl PreparedSourceArtV1 {
    pub fn request(&self) -> &SourceArtRequestV1 {
        &self.request
    }
    pub fn asset(&self) -> &AssetV4 {
        &self.asset
    }
    pub fn authority(&self) -> &TrustedJobAuthorityV1 {
        &self.authority
    }
    pub fn scope(&self) -> &str {
        &self.scope
    }
    pub fn policy(&self) -> &SourceArtPolicyV1 {
        &self.policy
    }
}

pub fn prepare_source_art(
    store: &impl SourceArtReadV1,
    request: &SourceArtRequestV1,
) -> Result<PreparedSourceArtV1, EditorError> {
    validate_request(request)?;
    let authorized = store.fixed_source_art_snapshot(&request.project_id, &request.revision)?;
    validate_policy(&authorized.policy)?;
    validation::scope(&authorized.scope)?;
    if authorized.snapshot.project.id != request.project_id
        || authorized.snapshot.revision != request.revision
    {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Source-art fixed scope differs",
        ));
    }
    opencut_editor_core::project_v4::validate_project_v4(&authorized.snapshot.project)?;
    let project = &authorized.snapshot.project;
    let asset = project
        .assets
        .iter()
        .find(|a| a.id == request.asset_id)
        .ok_or_else(|| EditorError::new("NOT_FOUND", "Fixed source-art asset does not exist"))?;
    if asset.version.size > authorized.policy.max_encoded_bytes {
        return Err(budget("Encoded source exceeds source-art admission"));
    }
    let element_clock = match &request.query {
        SourceArtQueryV1::Waveform { projection, .. } => {
            if !matches!(asset.kind, AssetKind::Audio | AssetKind::Video) {
                return Err(invalid("Source peaks require an audio or video resource"));
            }
            if matches!(asset.metadata.audio, AudioPresenceV4::Absent {}) {
                return Err(EditorError::new(
                    "NO_AUDIO_TRACK",
                    "Source resource is known to have no audio",
                ));
            }
            if let WaveformProjectionV1::Element {
                sequence_id,
                track_id,
                element_id,
                local_range,
                ..
            } = projection
            {
                let element = project
                    .sequences
                    .iter()
                    .find(|s| s.id == *sequence_id)
                    .and_then(|s| s.tracks.iter().find(|t| t.id == *track_id))
                    .and_then(|t| t.elements.iter().find(|e| e.id == *element_id))
                    .ok_or_else(|| {
                        EditorError::new("NOT_FOUND", "Source-art element scope does not exist")
                    })?;
                let (bound, retime) = match &element.content {
                    ElementContentV4::Video {
                        asset_id, retime, ..
                    }
                    | ElementContentV4::Audio {
                        asset_id, retime, ..
                    } => (asset_id, retime),
                    _ => {
                        return Err(invalid(
                            "Element source peaks require audio or video content",
                        ));
                    }
                };
                if bound != &request.asset_id {
                    return Err(invalid(
                        "Element source art differs from its fixed resource",
                    ));
                }
                let end = RationalTime::try_from(&local_range.start)?
                    .checked_add(RationalTime::try_from(&local_range.duration)?)?;
                if end.cmp_exact(RationalTime::try_from(&element.timeline_range.duration)?)
                    == Ordering::Greater
                {
                    return Err(invalid("Source-art local window exceeds element placement"));
                }
                let source = opencut_editor_core::project_v4::resolve_element_source_request_v4(
                    element,
                    Some(asset),
                )?;
                let rate = opencut_composition::project_v4::classic_effective_rate_v4(retime.rate)
                    .map_err(|e| EditorError::new(e.code, e.message))?;
                Some(ElementClock {
                    source_start: RationalTime::try_from(&source.start)?,
                    rate,
                })
            } else {
                None
            }
        }
        SourceArtQueryV1::Thumbnail {} => {
            if !matches!(asset.kind, AssetKind::Video | AssetKind::Image) {
                return Err(invalid("Thumbnail requires a video or image resource"));
            }
            None
        }
    };
    store.revalidate_source_art(
        &request.project_id,
        &request.revision,
        &authorized.authority,
    )?;
    Ok(PreparedSourceArtV1 {
        request: request.clone(),
        asset: asset.clone(),
        authority: authorized.authority,
        scope: authorized.scope,
        policy: authorized.policy,
        element_clock,
    })
}

pub fn validate_decoded_source_art(
    prepared: &PreparedSourceArtV1,
    decoded: &DecodedSourceArtV1,
) -> Result<(), EditorError> {
    validation::decoded(prepared, decoded)
}

pub fn project_source_art(
    prepared: &PreparedSourceArtV1,
    decoded: &DecodedSourceArtV1,
) -> Result<SourceArtResponseV1, EditorError> {
    validate_decoded_source_art(prepared, decoded)?;
    let data = match decoded {
        DecodedSourceArtV1::Waveform {
            sample_rate,
            channels,
            total_samples,
            bucket_size,
            amplitudes,
            ..
        } => {
            let SourceArtQueryV1::Waveform { projection, .. } = &prepared.request.query else {
                return Err(receipt("Source-art decoded kind differs"));
            };
            SourceArtDataV1::Waveform {
                sample_rate: *sample_rate,
                channels: *channels,
                total_samples: total_samples.to_string(),
                bucket_size: *bucket_size,
                projection: projection.clone(),
                amplitudes: projection::project(
                    prepared,
                    *sample_rate,
                    *total_samples,
                    amplitudes,
                )?,
            }
        }
        DecodedSourceArtV1::ThumbnailReady {
            width,
            height,
            jpeg_bytes,
            observation,
            ..
        } => {
            let mut jpeg_bytes_hex = String::with_capacity(jpeg_bytes.len() * 2);
            use std::fmt::Write;
            for byte in jpeg_bytes {
                write!(jpeg_bytes_hex, "{byte:02x}")
                    .map_err(|_| receipt("Could not encode source-art bytes"))?;
            }
            SourceArtDataV1::ThumbnailReady {
                width: *width,
                height: *height,
                content_hash: format!("{:x}", Sha256::digest(jpeg_bytes)),
                jpeg_bytes_hex,
                observation: observation.clone(),
            }
        }
        DecodedSourceArtV1::ThumbnailNoFrame {
            observation,
            reason,
            ..
        } => SourceArtDataV1::ThumbnailNoFrame {
            observation: observation.clone(),
            reason: *reason,
        },
    };
    Ok(SourceArtResponseV1 {
        schema_version: 1,
        project_id: prepared.request.project_id.clone(),
        revision: prepared.request.revision.clone(),
        asset_id: prepared.request.asset_id.clone(),
        version: prepared.asset.version.clone(),
        identity: decoded.identity().clone(),
        data,
    })
}

/// Raw summaries can be shared across fixed revisions in one trusted scope.
/// Projection and request/waiter identity are excluded; all execution budgets,
/// exact CAS version and runtime/decoder profile remain in the key.
pub fn source_art_decode_key(
    prepared: &PreparedSourceArtV1,
    identity: &SourceArtIdentityV1,
) -> Result<String, EditorError> {
    validation::identity(prepared, identity)?;
    let sample_rate = match prepared.request.query {
        SourceArtQueryV1::Waveform { sample_rate, .. } => Some(sample_rate),
        SourceArtQueryV1::Thumbnail {} => None,
    };
    let value = serde_json::json!({
        "schema_version": 1, "scope": prepared.scope,
        "version": prepared.asset.version, "asset_kind": prepared.asset.kind,
        "metadata": prepared.asset.metadata, "duration": prepared.asset.duration,
        "identity": identity, "sample_rate": sample_rate,
        "max_encoded_bytes": prepared.policy.max_encoded_bytes,
        "max_pcm_bytes": prepared.policy.max_pcm_bytes,
        "max_rss_bytes": prepared.policy.max_rss_bytes,
        "deadline_ms": prepared.policy.deadline_ms,
    });
    let bytes = serde_json::to_vec(&value)
        .map_err(|_| receipt("Could not identify source-art execution"))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
