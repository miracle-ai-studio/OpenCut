//! Typed bounded source evidence across all public analysis job kinds.
use crate::{JobState, job_views, queries::JobRead};
use opencut_api_contracts::analysis_evidence::{
    AnalysisEvidenceInput, AnalysisEvidenceItem, AnalysisEvidencePage, AnalysisParameters,
    AnalysisProvenance,
};
use opencut_contracts::{EditorError, Time, TimeRange};
use opencut_media_contracts::WordTimingStatus;
use opencut_time::{RationalTime, range_end, validate_range};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::cmp::Ordering;

fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
fn bad_result() -> EditorError {
    EditorError::new("INVALID_STORED_RESULT", "Analysis evidence is unavailable")
}
fn public_time(time: &opencut_media_contracts::MediaTime) -> Time {
    Time {
        ticks: time.ticks.to_string(),
        timebase: time.timebase,
    }
}
fn interval(
    start: &opencut_media_contracts::MediaTime,
    end: &opencut_media_contracts::MediaTime,
) -> Result<TimeRange, EditorError> {
    let a = RationalTime::new(start.ticks, start.timebase)?;
    let b = RationalTime::new(end.ticks, end.timebase)?;
    if b.cmp_exact(a) != Ordering::Greater {
        return Err(bad_result());
    }
    Ok(TimeRange {
        start: Time::from(a),
        duration: Time::from(b.checked_sub(a)?),
    })
}
fn window(
    input: &AnalysisEvidenceInput,
) -> Result<Option<(RationalTime, RationalTime)>, EditorError> {
    input
        .source_range
        .as_ref()
        .map(|range| {
            validate_range(range, true)?;
            let start = RationalTime::try_from(&range.start)?;
            if start.ticks() < 0 {
                return Err(invalid("Source range starts before zero"));
            }
            Ok((start, RationalTime::try_from(&range_end(range)?)?))
        })
        .transpose()
}
fn keep_range(
    range: &TimeRange,
    window: Option<(RationalTime, RationalTime)>,
) -> Result<bool, EditorError> {
    let Some((from, to)) = window else {
        return Ok(true);
    };
    let start = RationalTime::try_from(&range.start)?;
    let end = RationalTime::try_from(&range_end(range)?)?;
    Ok(end.cmp_exact(from) == Ordering::Greater && start.cmp_exact(to) == Ordering::Less)
}
fn keep_point(
    time: &Time,
    window: Option<(RationalTime, RationalTime)>,
) -> Result<bool, EditorError> {
    let Some((from, to)) = window else {
        return Ok(true);
    };
    let at = RationalTime::try_from(time)?;
    Ok(at.cmp_exact(from) != Ordering::Less && at.cmp_exact(to) == Ordering::Less)
}
fn result_data(
    view: &opencut_api_contracts::job_views::JobView,
) -> Result<&opencut_api_contracts::job_results::AssetAnalysisResult, EditorError> {
    if view.state != JobState::Succeeded {
        return Err(EditorError::new(
            "ANALYSIS_NOT_READY",
            "Analysis must succeed before evidence can be queried",
        ));
    }
    if view.result_unavailable.is_some() {
        return Err(bad_result());
    }
    match &view.result {
        Some(opencut_api_contracts::job_results::JobResultView::AssetAnalysis(result))
            if view.kind == "analysis.asset" =>
        {
            Ok(result)
        }
        _ => Err(bad_result()),
    }
}
fn digest<T: Serialize>(value: &T) -> Result<String, EditorError> {
    let bytes = serde_json::to_vec(value)
        .map_err(|_| EditorError::new("SERIALIZATION_ERROR", "Could not bind analysis cursor"))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub fn query(
    store: &impl JobRead,
    input: AnalysisEvidenceInput,
) -> Result<AnalysisEvidencePage, EditorError> {
    if !(1..=100).contains(&input.limit) || input.text.as_ref().is_some_and(|s| s.len() > 512) {
        return Err(invalid(
            "limit must be 1..100 and search text at most 512 UTF-8 bytes",
        ));
    }
    let text = input.text.as_deref().unwrap_or("").trim().to_lowercase();
    let selected_range = window(&input)?;
    // Job content, project read permission, and cursor principal are captured together.
    let (record, principal) = store
        .read_job_for_cursor(&input.job_id)
        .map_err(job_views::public_error)?;
    let view = job_views::project(record);
    let data = result_data(&view)?;
    let target = view.target.as_ref().ok_or_else(bad_result)?;
    let (asset_id, asset_version, provenance, mut evidence): (
        String,
        opencut_contracts::LockedAsset,
        AnalysisProvenance,
        Vec<AnalysisEvidenceItem>,
    );
    use crate::asset_analysis::{AssetAnalysisData, AssetAnalysisOptions};
    let result = data;
    if result.project_id != target.project_id || result.revision != target.revision {
        return Err(bad_result());
    }
    asset_id = result.asset_id.clone();
    asset_version = result.asset_version.clone();
    match (&result.options, &result.data) {
        (
            AssetAnalysisOptions::Transcribe { language, .. },
            AssetAnalysisData::Transcribe(value),
        ) => {
            provenance = AnalysisProvenance {
                provider: value.provider.clone(),
                method: None,
                model: Some(value.model.clone()),
                model_sha256: value.model_sha256.clone(),
                confidence_known: Some(false),
                speaker_identity_status: None,
                parameters: AnalysisParameters::Transcription {
                    language: language.clone(),
                    word_timing_status: match value.word_timing_status {
                        WordTimingStatus::Available => "available",
                        WordTimingStatus::Unavailable => "unavailable",
                        WordTimingStatus::NotRequested => "not_requested",
                    }
                    .into(),
                },
            };
            evidence = value
                .segments
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    Ok(AnalysisEvidenceItem::TranscriptSegment {
                        evidence_id: format!("{}:segment:{i}", input.job_id),
                        index: i as u32,
                        source_range: interval(&s.start, &s.end)?,
                        text: s.text.clone(),
                    })
                })
                .collect::<Result<_, EditorError>>()?;
        }
        (
            AssetAnalysisOptions::Silence {
                noise_db,
                min_duration_ms,
            },
            AssetAnalysisData::Silence(value),
        ) => {
            provenance = AnalysisProvenance {
                provider: value.provider.clone(),
                method: Some(value.method.clone()),
                model: None,
                model_sha256: None,
                confidence_known: Some(value.confidence_known),
                speaker_identity_status: None,
                parameters: AnalysisParameters::Silence {
                    noise_db: *noise_db,
                    min_duration_ms: *min_duration_ms,
                },
            };
            evidence = value
                .intervals
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    Ok(AnalysisEvidenceItem::SilenceInterval {
                        evidence_id: format!("{}:silence:{i}", input.job_id),
                        index: i as u32,
                        source_range: interval(&s.start, &s.end)?,
                    })
                })
                .collect::<Result<_, EditorError>>()?;
        }
        (AssetAnalysisOptions::Shots { threshold }, AssetAnalysisData::Shots(value)) => {
            provenance = AnalysisProvenance {
                provider: value.provider.clone(),
                method: Some(value.method.clone()),
                model: None,
                model_sha256: None,
                confidence_known: Some(value.confidence_known),
                speaker_identity_status: None,
                parameters: AnalysisParameters::Shots {
                    threshold: *threshold,
                },
            };
            evidence = value
                .boundaries
                .iter()
                .enumerate()
                .map(|(i, s)| AnalysisEvidenceItem::ShotBoundary {
                    evidence_id: format!("{}:shot:{i}", input.job_id),
                    index: i as u32,
                    source_time: public_time(&s.at),
                    scene_score: s.scene_score,
                })
                .collect();
        }
        (AssetAnalysisOptions::Waveform { buckets }, AssetAnalysisData::Waveform(value)) => {
            provenance = AnalysisProvenance {
                provider: value.provider.clone(),
                method: Some(value.method.clone()),
                model: None,
                model_sha256: None,
                confidence_known: Some(value.confidence_known),
                speaker_identity_status: None,
                parameters: AnalysisParameters::Waveform { buckets: *buckets },
            };
            evidence = value
                .buckets
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    Ok(AnalysisEvidenceItem::WaveformBucket {
                        evidence_id: format!("{}:waveform:{i}", input.job_id),
                        index: i as u32,
                        source_range: interval(&s.start, &s.end)?,
                        rms: s.rms,
                        peak: s.peak,
                    })
                })
                .collect::<Result<_, EditorError>>()?;
        }
        (AssetAnalysisOptions::SpeakerTurns {}, AssetAnalysisData::SpeakerTurns(value)) => {
            provenance = AnalysisProvenance {
                provider: value.provider.clone(),
                method: Some(value.method.clone()),
                model: Some(value.model.clone()),
                model_sha256: Some(value.model_sha256.clone()),
                confidence_known: Some(value.confidence_known),
                speaker_identity_status: Some(value.speaker_identity_status.clone()),
                parameters: AnalysisParameters::SpeakerTurns {
                    language: "en".into(),
                },
            };
            evidence = value
                .turns
                .iter()
                .enumerate()
                .map(|(i, s)| AnalysisEvidenceItem::SpeakerTurn {
                    evidence_id: format!("{}:turn:{i}", input.job_id),
                    index: i as u32,
                    source_time: public_time(&s.at),
                })
                .collect();
        }
        _ => return Err(bad_result()),
    }
    if asset_version.asset_id != asset_id {
        return Err(bad_result());
    }
    if !text.is_empty()
        && !matches!(
            provenance.parameters,
            AnalysisParameters::Transcription { .. }
        )
    {
        return Err(invalid(
            "Text matching is only available for transcription evidence",
        ));
    }
    evidence = evidence
        .into_iter()
        .map(|item| {
            let selected = match &item {
                AnalysisEvidenceItem::TranscriptSegment {
                    source_range,
                    text: segment,
                    ..
                } => keep_range(source_range, selected_range)
                    .map(|overlap| overlap && segment.to_lowercase().contains(&text)),
                AnalysisEvidenceItem::SilenceInterval { source_range, .. }
                | AnalysisEvidenceItem::WaveformBucket { source_range, .. } => {
                    keep_range(source_range, selected_range)
                }
                AnalysisEvidenceItem::ShotBoundary { source_time, .. }
                | AnalysisEvidenceItem::SpeakerTurn { source_time, .. } => {
                    keep_point(source_time, selected_range)
                }
            };
            selected.map(|keep| keep.then_some(item))
        })
        .collect::<Result<Vec<_>, EditorError>>()?
        .into_iter()
        .flatten()
        .collect();
    let identity = digest(&(
        input.job_id.as_str(),
        view.kind.as_str(),
        text.as_str(),
        &input.source_range,
        &data,
        &principal,
    ))?;
    let offset = if let Some(cursor) = input.cursor.as_deref() {
        let prefix = format!("evidence:{identity}:");
        if cursor.len() > 256 {
            return Err(EditorError::new(
                "INVALID_CURSOR",
                "Analysis cursor is invalid",
            ));
        }
        cursor
            .strip_prefix(&prefix)
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|o| *o <= evidence.len())
            .ok_or_else(|| {
                EditorError::new(
                    "INVALID_CURSOR",
                    "Cursor does not match analysis, query or caller",
                )
            })?
    } else {
        0
    };
    let end = (offset + input.limit).min(evidence.len());
    let next_cursor = (end < evidence.len()).then(|| format!("evidence:{identity}:{end}"));
    Ok(AnalysisEvidencePage {
        analysis_job_id: input.job_id,
        analysis_kind: view.kind,
        project_id: target.project_id.clone(),
        revision: target.revision.clone(),
        asset_id,
        asset_version,
        provenance,
        evidence: evidence[offset..end].to_vec(),
        next_cursor,
    })
}
