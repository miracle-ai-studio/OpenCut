//! Read-only pagination; authorization happens at the existing JobRead port.
use crate::{
    JobState,
    queries::{self, JobRead},
};
use opencut_api_contracts::analysis_query::*;
use opencut_contracts::{EditorError, Time, TimeRange};
use opencut_job_contracts::TranscribeJobResult;
use opencut_time::{RationalTime, range_end, validate_range};
use std::cmp::Ordering;

fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}

pub fn query(store: &impl JobRead, input: AnalysisQueryInput) -> Result<AnalysisPage, EditorError> {
    // Read authorization precedes result and cursor inspection.
    let view = queries::job_view(store, &input.job_id)?;
    if view.state != JobState::Succeeded {
        return Err(EditorError::new(
            "ANALYSIS_NOT_READY",
            "Analysis must have succeeded before evidence can be queried",
        ));
    }
    if view.kind != "analysis.transcribe" {
        return Err(EditorError::new(
            "UNSUPPORTED_ANALYSIS_QUERY",
            "Text evidence queries require a transcription job",
        ));
    }
    if view.result_unavailable.is_some() || view.result.is_some() {
        return Err(EditorError::new(
            "INVALID_STORED_RESULT",
            "Analysis result is unavailable",
        ));
    }
    let result = view
        .extended_result
        .ok_or_else(|| invalid("Analysis result missing"))?;
    if result.kind != view.kind || result.schema_version != 1 {
        return Err(EditorError::new(
            "UNSUPPORTED_RESULT_VERSION",
            "Unsupported analysis result version",
        ));
    }
    let transcript: TranscribeJobResult = serde_json::from_value(result.data)
        .map_err(|_| EditorError::new("INVALID_STORED_RESULT", "Invalid transcription result"))?;
    page(input, transcript)
}

fn page(
    input: AnalysisQueryInput,
    transcript: TranscribeJobResult,
) -> Result<AnalysisPage, EditorError> {
    if !(1..=100).contains(&input.limit) || input.text.as_ref().is_some_and(|text| text.len() > 512)
    {
        return Err(invalid(
            "limit must be 1..100 and search text at most 512 UTF-8 bytes",
        ));
    }
    let text = input.text.as_deref().unwrap_or("").trim().to_lowercase();
    let window = input
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
        .transpose()?;
    let fingerprint = crate::identity::request_identity(
        "analysis_query",
        &input.job_id,
        &(text.as_str(), &input.source_range, &transcript),
    )?;
    let identity = fingerprint.current_digest();
    let start_index = match input.cursor.as_deref() {
        None => 0,
        Some(cursor) => {
            let prefix = format!("evidence:{identity}:");
            cursor
                .strip_prefix(&prefix)
                .and_then(|offset| offset.parse::<usize>().ok())
                .filter(|offset| *offset <= transcript.transcription.segments.len())
                .ok_or_else(|| {
                    EditorError::new("INVALID_CURSOR", "Cursor does not match analysis and query")
                })?
        }
    };
    let mut evidence = Vec::with_capacity(input.limit);
    let mut next_cursor = None;
    for (index, segment) in transcript
        .transcription
        .segments
        .iter()
        .enumerate()
        .skip(start_index)
    {
        let start = RationalTime::new(segment.start.ticks, segment.start.timebase)?;
        let end = RationalTime::new(segment.end.ticks, segment.end.timebase)?;
        if !segment.text.to_lowercase().contains(&text)
            || window.is_some_and(|(from, to)| {
                end.cmp_exact(from) != Ordering::Greater || start.cmp_exact(to) != Ordering::Less
            })
        {
            continue;
        }
        if evidence.len() == input.limit {
            next_cursor = Some(format!("evidence:{identity}:{index}"));
            break;
        }
        evidence.push(AnalysisEvidence {
            evidence_id: format!("{}:segment:{index}", input.job_id),
            segment_index: index as u32,
            source_range: TimeRange {
                start: Time::from(start),
                duration: end.checked_sub(start)?.into(),
            },
            text: segment.text.clone(),
        });
    }
    Ok(AnalysisPage {
        analysis_job_id: input.job_id,
        project_id: transcript.project_id,
        revision: transcript.revision,
        asset_id: transcript.asset_id,
        asset_version: transcript.asset_version,
        provider: transcript.transcription.provider,
        model: transcript.transcription.model,
        model_sha256: transcript.transcription.model_sha256,
        evidence,
        next_cursor,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn transcript() -> TranscribeJobResult {
        serde_json::from_value(json!({"project_id":"p","revision":"r","asset_id":"a",
            "asset_version":{"asset_id":"a","content_hash":"a".repeat(64),"size":42},
            "transcription":{"provider":"whisper.cpp","model":"tiny","language":"zh","segments":[
                {"start":{"ticks":"0","timebase":1000},"end":{"ticks":"1000","timebase":1000},"text":"第一段视频","words":[]},
                {"start":{"ticks":"1000","timebase":1000},"end":{"ticks":"2000","timebase":1000},"text":"第二段视频","words":[]}]}})).unwrap()
    }
    fn request() -> AnalysisQueryInput {
        AnalysisQueryInput {
            job_id: "job_example".into(),
            text: Some("视频".into()),
            source_range: None,
            cursor: None,
            limit: 1,
        }
    }
    #[test]
    fn pages_source_evidence_without_duplicates_and_binds_cursor_to_query() {
        let first = page(request(), transcript()).unwrap();
        assert_eq!(first.evidence[0].segment_index, 0);
        let mut next = request();
        next.cursor = first.next_cursor;
        let second = page(next.clone(), transcript()).unwrap();
        assert_eq!(second.evidence[0].segment_index, 1);
        assert!(second.next_cursor.is_none());
        next.text = Some("不同".into());
        assert_eq!(page(next, transcript()).unwrap_err().code, "INVALID_CURSOR");
    }
    #[test]
    fn source_windows_use_exact_half_open_intersections() {
        let mut input = request();
        input.source_range = Some(TimeRange {
            start: Time {
                ticks: "2".into(),
                timebase: 2,
            },
            duration: Time {
                ticks: "1".into(),
                timebase: 2,
            },
        });
        let result = page(input.clone(), transcript()).unwrap();
        assert_eq!(result.evidence[0].segment_index, 1);
        input.limit = 0;
        assert!(page(input, transcript()).is_err());
    }
}
