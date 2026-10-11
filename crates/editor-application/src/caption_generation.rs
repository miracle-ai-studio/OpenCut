//! Transcript-to-captions planning. Source inference and persistence are separate ports.
use opencut_api_contracts::subtitle_exchange::GenerateCaptions;
use opencut_contracts::{
    CaptionCue, CaptionStyle, EditorError, FontResolutionPolicy, TextAlign, Time, TimeRange,
    project_v3::ProjectSnapshotV3, project_v3_ops::VisualPlan,
};
use opencut_job_contracts::SequenceTranscribeResult;
use opencut_time::RationalTime;

pub fn plan(
    snapshot: &ProjectSnapshotV3,
    input: &GenerateCaptions,
    result: &SequenceTranscribeResult,
) -> Result<VisualPlan, EditorError> {
    if snapshot.project.base.id != input.project_id
        || snapshot.revision != input.expected_revision
        || result.project_id != input.project_id
        || result.revision != input.expected_revision
        || result.sequence_id != input.sequence_id
    {
        return Err(EditorError::new(
            "REVISION_CONFLICT",
            "Captions require the original transcribed scene revision",
        ));
    }
    if !(1..=20).contains(&input.words_per_caption) {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Words per caption must be between 1 and 20",
        ));
    }
    if !opencut_media_contracts::validate_public_transcription_result(&result.transcription) {
        return Err(EditorError::new(
            "INVALID_STORED_RESULT",
            "Transcript is invalid",
        ));
    }
    let end = RationalTime::try_from(&result.duration)?;
    if end.ticks() <= 0 {
        return Err(EditorError::new(
            "INVALID_STORED_RESULT",
            "Transcript duration is invalid",
        ));
    }
    let style = input.style.clone().unwrap_or_else(|| CaptionStyle {
        font_family: "Arial".into(),
        font_size: 24.0,
        color: "#ffffff".into(),
        background_color: None,
        font_policy: FontResolutionPolicy::Strict {},
        max_width: None,
        line_height: None,
        text_align: TextAlign::Center,
    });
    let mut cues = Vec::new();
    for segment in &result.transcription.segments {
        let words: Vec<_> = segment.text.split_whitespace().collect();
        let duration = segment.end.ticks - segment.start.ticks;
        for (index, chunk) in words
            .chunks(usize::from(input.words_per_caption))
            .enumerate()
        {
            let first = index * usize::from(input.words_per_caption);
            let next = first + chunk.len();
            let start = segment.start.ticks
                + (i128::from(duration) * first as i128 / words.len() as i128) as i64;
            let stop = segment.start.ticks
                + (i128::from(duration) * next as i128 / words.len() as i128) as i64;
            let from = RationalTime::new(start, 1000)?;
            let to = RationalTime::new(stop, 1000)?;
            // Whisper can emit a final interval extending beyond the recording.
            // Intersect it with the exact scene duration; never extend the scene.
            if from.cmp_exact(end).is_ge() {
                continue;
            }
            let to = if to.cmp_exact(end).is_gt() { end } else { to };
            if to.cmp_exact(from).is_le() {
                continue;
            }
            if cues.len() == 5000 {
                return Err(EditorError::new(
                    "CAPTION_LIMIT",
                    "Generated captions exceed the 5000 cue limit",
                ));
            }
            let cue = CaptionCue {
                id: crate::content_id(
                    "cue",
                    format!(
                        "generated:{}:{}:{}",
                        input.project_id,
                        input.idempotency_key,
                        cues.len()
                    )
                    .as_bytes(),
                ),
                timeline_range: TimeRange {
                    start: Time::from(from),
                    duration: Time::from(to.checked_sub(from)?),
                },
                text: chunk.join(" "),
                source_anchor: None,
                style: style.clone(),
            };
            cues.push(cue);
        }
    }
    if cues.is_empty() {
        return Err(EditorError::new(
            "NO_SPEECH",
            "No speech captions were found in the scene",
        ));
    }
    let mut proposed_project = snapshot.project.clone();
    let sequence = proposed_project
        .base
        .sequences
        .iter_mut()
        .find(|scene| scene.id == input.sequence_id)
        .ok_or_else(|| EditorError::new("NOT_FOUND", "Sequence does not exist"))?;
    let created_ids = cues.iter().map(|cue| cue.id.clone()).collect();
    sequence.caption_cues.extend(cues);
    // Validate the complete bounded proposal once. The public edit tool's
    // operation limit stays frozen; storage publishes one ordinary journal entry.
    opencut_editor_core::project_v3::validate_project_v3(&proposed_project)?;
    Ok(VisualPlan {
        proposed_project,
        created_ids,
        changed_ids: vec![input.sequence_id.clone()],
        deleted_ids: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn fixtures() -> (
        ProjectSnapshotV3,
        GenerateCaptions,
        SequenceTranscribeResult,
    ) {
        let project = crate::project_v3::create(
            &serde_json::from_value(json!({
            "id":"p","name":"Speech","sequence_id":"scene","width":640,"height":360,
            "fps":{"numerator":30000,"denominator":1001}}))
            .unwrap(),
        )
        .unwrap();
        let input: GenerateCaptions = serde_json::from_value(json!({"project_id":"p","expected_revision":"r",
            "sequence_id":"scene","analysis_job_id":"job","idempotency_key":"captions","words_per_caption":3,"style":null})).unwrap();
        let result: SequenceTranscribeResult = serde_json::from_value(json!({"project_id":"p","revision":"r","sequence_id":"scene",
            "duration":{"ticks":"5005","timebase":1000},"asset_lock":{"version":1,"revision":"r","sequence_id":"scene","assets":[]},
            "transcription":{"language":"en","provider":"whisper.cpp","model":"small","segments":[
                {"start":{"ticks":"1000","timebase":1000},"end":{"ticks":"7000","timebase":1000},"text":"one two three four five six","words":[]}]
            }})).unwrap();
        (
            ProjectSnapshotV3 {
                project,
                revision: "r".into(),
            },
            input,
            result,
        )
    }
    #[test]
    fn estimates_bounded_timeline_cues_and_deterministic_identity() {
        let (snapshot, input, result) = fixtures();
        let output = plan(&snapshot, &input, &result).unwrap();
        let cues = &output.proposed_project.base.sequences[0].caption_cues;
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].timeline_range.start.ticks, "1000");
        assert_eq!(cues[0].timeline_range.duration.ticks, "3000");
        assert_eq!(cues[1].timeline_range.start.ticks, "4000");
        assert_eq!(cues[1].timeline_range.duration.ticks, "1005");
        assert_eq!(cues[0].text, "one two three");
        assert!(cues.iter().all(|cue| cue.source_anchor.is_none()));
        assert_eq!(plan(&snapshot, &input, &result).unwrap(), output);
    }
    #[test]
    fn rejects_stale_foreign_results_invalid_style_and_empty_speech_without_partial_edit() {
        let (mut snapshot, mut input, mut result) = fixtures();
        snapshot.revision = "new".into();
        assert_eq!(
            plan(&snapshot, &input, &result).unwrap_err().code,
            "REVISION_CONFLICT"
        );
        snapshot.revision = "r".into();
        result.sequence_id = "other".into();
        assert_eq!(
            plan(&snapshot, &input, &result).unwrap_err().code,
            "REVISION_CONFLICT"
        );
        result.sequence_id = "scene".into();
        input.words_per_caption = 0;
        assert_eq!(
            plan(&snapshot, &input, &result).unwrap_err().code,
            "INVALID_ARGUMENT"
        );
        input.words_per_caption = 3;
        let mut style: CaptionStyle = serde_json::from_value(
            json!({"font_family":"Arial","font_size":0,"color":"#ffffff","background_color":null}),
        )
        .unwrap();
        input.style = Some(style.clone());
        assert!(plan(&snapshot, &input, &result).is_err());
        style.font_size = 24.0;
        input.style = Some(style);
        result.transcription.segments.clear();
        assert_eq!(
            plan(&snapshot, &input, &result).unwrap_err().code,
            "NO_SPEECH"
        );
        assert!(snapshot.project.base.sequences[0].caption_cues.is_empty());
    }
    #[test]
    fn supports_long_transcripts_in_one_plan_and_rejects_bulk_overflow() {
        let (snapshot, mut input, mut result) = fixtures();
        input.words_per_caption = 1;
        let segment = &mut result.transcription.segments[0];
        segment.text = vec!["word"; 129].join(" ");
        result.duration.ticks = "10000".into();
        assert_eq!(
            plan(&snapshot, &input, &result).unwrap().created_ids.len(),
            129
        );
        let template = result.transcription.segments[0].clone();
        result.transcription.segments = (0..11)
            .map(|index| {
                let mut segment = template.clone();
                segment.start.ticks = index * 1000;
                segment.end.ticks = (index + 1) * 1000;
                segment.text = vec!["word"; 500].join(" ");
                segment
            })
            .collect();
        result.duration.ticks = "11000".into();
        assert_eq!(
            plan(&snapshot, &input, &result).unwrap_err().code,
            "CAPTION_LIMIT"
        );
        input.words_per_caption = 3;
        assert_eq!(
            plan(&snapshot, &input, &result).unwrap().created_ids.len(),
            1837
        );
    }
}
