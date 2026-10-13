//! Deterministic captions owned by V4 caption tracks. Timing estimates are explicit.
use opencut_api_contracts::{subtitle_exchange::SubtitleExportReceipt, subtitle_v4::*};
use opencut_contracts::{EditorError, Time, TimeRange, project_v4::*, project_v4_ops::*};
use opencut_job_contracts::sequence_transcribe_v4::SequenceTranscribeResultV4;
use opencut_time::{RationalTime, range_end};
use std::collections::BTreeSet;
const MAX_CUES: usize = 5000;
fn invalid(message: &str) -> EditorError {
    EditorError::new("SUBTITLE_INVALID", message)
}
fn sequence<'a>(snapshot: &'a ProjectSnapshotV4, id: &str) -> Result<&'a SequenceV4, EditorError> {
    snapshot
        .project
        .sequences
        .iter()
        .find(|s| s.id == id)
        .ok_or_else(|| EditorError::new("NOT_FOUND", "Caption scene does not exist"))
}
fn owning_track(
    snapshot: &ProjectSnapshotV4,
    scene: &str,
    track: &str,
    style: &CaptionStyleV4,
) -> Result<(), EditorError> {
    let s = sequence(snapshot, scene)?;
    let t = s
        .tracks
        .iter()
        .find(|t| t.id == track)
        .ok_or_else(|| EditorError::new("NOT_FOUND", "Caption track does not exist"))?;
    if t.kind != TrackKindV4::Caption {
        return Err(invalid("Captions require an owning caption track"));
    }
    if t.locked {
        return Err(EditorError::new("TRACK_LOCKED", "Caption track is locked"));
    }
    if style.layout != CaptionLayoutProfileV4::PinnedCaptionV4 {
        return Err(invalid(
            "Generated and imported captions require pinned V4 layout",
        ));
    }
    Ok(())
}
fn edit(
    snapshot: &ProjectSnapshotV4,
    scene: &str,
    track: &str,
    key: &str,
    cues: Vec<CaptionV4>,
    label: &str,
) -> Result<ApplyV4Project, EditorError> {
    if cues.is_empty() {
        return Err(EditorError::new("NO_SPEECH", "No caption cues were found"));
    }
    if cues.len() > MAX_CUES {
        return Err(EditorError::new(
            "CAPTION_LIMIT",
            "Caption import exceeds 5000 cues",
        ));
    }
    let mut s = sequence(snapshot, scene)?.clone();
    if s.captions
        .len()
        .checked_add(cues.len())
        .is_none_or(|n| n > MAX_CUES)
    {
        return Err(EditorError::new(
            "CAPTION_LIMIT",
            "Scene exceeds 5000 captions",
        ));
    }
    if cues.iter().any(|c| c.track_id != track) {
        return Err(invalid("Caption ownership differs"));
    }
    s.captions.extend(cues);
    let input = ApplyV4Project {
        project_id: snapshot.project.id.clone(),
        expected_revision: snapshot.revision.clone(),
        idempotency_key: key.into(),
        label: label.into(),
        operations: vec![ProjectOperationV4::SequencePut { sequence: s }],
    };
    // Ordinary V4 planning includes global identity, font, owning track and lock rules.
    crate::project_v4::plan(snapshot, &input)?;
    Ok(input)
}
fn scope(snapshot: &ProjectSnapshotV4, p: &str, r: &str) -> Result<(), EditorError> {
    if snapshot.project.id != p || snapshot.revision != r {
        return Err(EditorError::new(
            "REVISION_CONFLICT",
            "Captions require their fixed source revision",
        ));
    }
    Ok(())
}
pub fn generate(
    snapshot: &ProjectSnapshotV4,
    input: &GenerateCaptionsV4,
    result: &SequenceTranscribeResultV4,
) -> Result<ApplyV4Project, EditorError> {
    scope(snapshot, &input.project_id, &input.expected_revision)?;
    if result.project_id != input.project_id
        || result.revision != input.expected_revision
        || result.sequence_id != input.sequence_id
    {
        return Err(EditorError::new(
            "REVISION_CONFLICT",
            "Captions require the original transcribed V4 scene",
        ));
    }
    owning_track(snapshot, &input.sequence_id, &input.track_id, &input.style)?;
    if !(1..=20).contains(&input.words_per_caption) {
        return Err(invalid("Words per caption must be between 1 and 20"));
    }
    if !opencut_media_contracts::validate_public_transcription_result(&result.transcription) {
        return Err(EditorError::new(
            "INVALID_STORED_RESULT",
            "Transcript is invalid",
        ));
    }
    let end = RationalTime::try_from(&result.duration)?;
    let mut cues = Vec::new();
    for (segment_index, segment) in result.transcription.segments.iter().enumerate() {
        let words: Vec<_> = segment.text.split_whitespace().collect();
        let duration = segment.end.ticks - segment.start.ticks;
        for (chunk_index, chunk) in words
            .chunks(usize::from(input.words_per_caption))
            .enumerate()
        {
            let first = chunk_index * usize::from(input.words_per_caption);
            let next = first + chunk.len();
            let from = RationalTime::new(
                segment.start.ticks
                    + (i128::from(duration) * first as i128 / words.len() as i128) as i64,
                1000,
            )?;
            let to = RationalTime::new(
                segment.start.ticks
                    + (i128::from(duration) * next as i128 / words.len() as i128) as i64,
                1000,
            )?;
            if from.cmp_exact(end).is_ge() {
                continue;
            }
            let to = if to.cmp_exact(end).is_gt() { end } else { to };
            if to.cmp_exact(from).is_le() {
                continue;
            }
            if cues.len() == MAX_CUES {
                return Err(EditorError::new(
                    "CAPTION_LIMIT",
                    "Generated captions exceed 5000 cues",
                ));
            }
            cues.push(CaptionV4 {
                id: crate::content_id(
                    "caption",
                    format!(
                        "v4:{}:{}:{}:{}",
                        input.project_id, input.idempotency_key, segment_index, chunk_index
                    )
                    .as_bytes(),
                ),
                track_id: input.track_id.clone(),
                timeline_range: TimeRange {
                    start: Time::from(from),
                    duration: Time::from(to.checked_sub(from)?),
                },
                text: chunk.join(" "),
                source_anchor: None,
                style: input.style.clone(),
            });
        }
    }
    edit(
        snapshot,
        &input.sequence_id,
        &input.track_id,
        &input.idempotency_key,
        cues,
        "Generate captions",
    )
}

pub fn import(
    snapshot: &ProjectSnapshotV4,
    input: &SubtitleImportV4,
) -> Result<ApplyV4Project, EditorError> {
    scope(snapshot, &input.project_id, &input.expected_revision)?;
    owning_track(snapshot, &input.sequence_id, &input.track_id, &input.style)?;
    if input.content.len() > 2 * 1024 * 1024 {
        return Err(invalid("Subtitle file exceeds 2 MiB"));
    }
    let fallback = opencut_contracts::CaptionStyle {
        font_family: "V4 pinned font".into(),
        font_size: 24.0,
        color: "#ffffff".into(),
        background_color: None,
        font_policy: opencut_contracts::FontResolutionPolicy::Strict {},
        max_width: None,
        line_height: None,
        text_align: opencut_contracts::TextAlign::Center,
    };
    let parsed = match input.format {
        SubtitleFormatV4::Srt => crate::subtitle_exchange::parse_srt(&input.content, &fallback)?,
        SubtitleFormatV4::Ass => crate::subtitle_exchange::parse_ass(&input.content, &fallback)?,
    };
    // Interchange carries text and intervals. Explicit pinned style is authoritative;
    // ASS family names cannot silently acquire a host font or unlicensed resource.
    let cues = parsed
        .into_iter()
        .enumerate()
        .map(|(i, c)| CaptionV4 {
            id: crate::content_id(
                "caption",
                format!(
                    "subtitle-v4:{}:{}:{}",
                    input.project_id, input.idempotency_key, i
                )
                .as_bytes(),
            ),
            track_id: input.track_id.clone(),
            timeline_range: c.timeline_range,
            text: c.text,
            source_anchor: None,
            style: input.style.clone(),
        })
        .collect();
    edit(
        snapshot,
        &input.sequence_id,
        &input.track_id,
        &input.idempotency_key,
        cues,
        "Import subtitles",
    )
}

fn units(time: &Time, base: i128) -> Result<(i64, bool), EditorError> {
    let t = RationalTime::try_from(time)?;
    if t.ticks() < 0 {
        return Err(invalid("Negative subtitle time"));
    }
    let n = i128::from(t.ticks())
        .checked_mul(base)
        .ok_or_else(|| invalid("Subtitle time overflow"))?;
    let d = i128::from(t.timebase());
    Ok((
        i64::try_from((n + d / 2) / d).map_err(|_| invalid("Subtitle time overflow"))?,
        n % d != 0,
    ))
}
fn clock(t: i64, ass: bool) -> String {
    let b = if ass { 100 } else { 1000 };
    let s = t / b;
    if ass {
        format!(
            "{}:{:02}:{:02}.{:02}",
            s / 3600,
            (s / 60) % 60,
            s % 60,
            t % b
        )
    } else {
        format!(
            "{:02}:{:02}:{:02},{:03}",
            s / 3600,
            (s / 60) % 60,
            s % 60,
            t % b
        )
    }
}
pub fn export(
    snapshot: &ProjectSnapshotV4,
    input: &SubtitleExportV4,
) -> Result<SubtitleExportReceipt, EditorError> {
    scope(snapshot, &input.project_id, &input.revision)?;
    let scene = sequence(snapshot, &input.sequence_id)?;
    if let Some(track) = &input.track_id {
        if !scene
            .tracks
            .iter()
            .any(|t| &t.id == track && t.kind == TrackKindV4::Caption)
        {
            return Err(EditorError::new(
                "NOT_FOUND",
                "Caption track does not exist",
            ));
        }
    }
    let mut cues = scene
        .captions
        .iter()
        .filter(|c| input.track_id.as_ref().is_none_or(|t| &c.track_id == t))
        .collect::<Vec<_>>();
    if cues.len() > MAX_CUES {
        return Err(invalid("Too many subtitle cues"));
    }
    cues.sort_by(|a, b| {
        RationalTime::try_from(&a.timeline_range.start)
            .unwrap()
            .cmp_exact(RationalTime::try_from(&b.timeline_range.start).unwrap())
            .then(a.id.cmp(&b.id))
    });
    let ass = input.format == SubtitleFormatV4::Ass;
    let mut losses = BTreeSet::new();
    let mut content = String::new();
    if ass {
        content.push_str("[Script Info]\nScriptType: v4.00+\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Arial,24,&H00FFFFFF,&H00FFFFFF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,0,0,2,0,0,0,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n");
        losses.insert("ASS export uses a portable default style; pinned V4 fonts, colour, layout and track ownership are not embedded".into());
    } else {
        losses.insert("SRT does not represent styles, pinned fonts or track ownership".into());
    }
    for (index, cue) in cues.iter().enumerate() {
        let (start, rs) = units(&cue.timeline_range.start, if ass { 100 } else { 1000 })?;
        let (end, re) = units(
            &range_end(&cue.timeline_range)?,
            if ass { 100 } else { 1000 },
        )?;
        if end <= start {
            return Err(invalid("Caption is shorter than subtitle precision"));
        }
        if rs || re {
            losses.insert("Timing rounded to interchange precision".into());
        }
        if cue.source_anchor.is_some() {
            losses.insert("Source anchors are not represented".into());
        }
        if ass {
            if cue.text.contains(['{', '}']) || cue.text.contains("\\N") || cue.text.contains("\\n")
            {
                return Err(invalid(
                    "Caption text contains ASS syntax that cannot be safely escaped",
                ));
            }
            content.push_str(&format!(
                "Dialogue: 0,{},{},Default,,0,0,0,,{}\n",
                clock(start, true),
                clock(end, true),
                cue.text.replace('\n', "\\N")
            ));
        } else {
            content.push_str(&format!(
                "{}\n{} --> {}\n{}\n\n",
                index + 1,
                clock(start, false),
                clock(end, false),
                cue.text
            ));
        }
    }
    if content.len() > 2 * 1024 * 1024 {
        return Err(invalid("Exported subtitle file exceeds 2 MiB"));
    }
    Ok(SubtitleExportReceipt {
        project_id: input.project_id.clone(),
        revision: input.revision.clone(),
        sequence_id: input.sequence_id.clone(),
        format: match input.format {
            SubtitleFormatV4::Srt => opencut_api_contracts::subtitle_exchange::SubtitleFormat::Srt,
            SubtitleFormatV4::Ass => opencut_api_contracts::subtitle_exchange::SubtitleFormat::Ass,
        },
        content,
        cue_count: cues.len(),
        losses: losses.into_iter().collect(),
    })
}

#[cfg(test)]
mod tests;
