//! One private transport compression. Expansion preserves accepted array
//! order and observations; existing application/pure bind remains authoritative.
use super::{error, ids};
use opencut_composition::scene::{PreviewAudioSegment, PreviewSourceFormat};
use opencut_contracts::EditorError;
use opencut_editor_application::classic_audio::decimal;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

const MAX_EXPANDED_BYTES: usize = 16 * 1024 * 1024;
const FORMAT_FIELDS: &[&str] = &[
    "elementId",
    "sampleRate",
    "frameCount",
    "channels",
    "prepared",
    "sourceRef",
];
fn invalid() -> EditorError {
    error(
        "AUDIO_PROTOCOL_ERROR",
        "Invalid shared preview source binding",
    )
}
fn budget() -> EditorError {
    error(
        "AUDIO_BUDGET_EXCEEDED",
        "Expanded preview facts exceed the 16 MiB budget",
    )
}
fn object<'a>(value: &'a Value, keys: &[&str]) -> Result<&'a Map<String, Value>, EditorError> {
    let fields = value.as_object().ok_or_else(invalid)?;
    if fields.len() != keys.len() || keys.iter().any(|key| !fields.contains_key(*key)) {
        return Err(invalid());
    }
    Ok(fields)
}
fn identifier(value: &Value) -> Result<&str, EditorError> {
    value
        .as_str()
        .filter(|id| !id.trim().is_empty() && id.len() <= 1024 && !id.chars().any(char::is_control))
        .ok_or_else(invalid)
}
fn bounded_array(value: &Value, maximum: usize) -> Result<&[Value], EditorError> {
    value
        .as_array()
        .filter(|values| values.len() <= maximum)
        .map(Vec::as_slice)
        .ok_or_else(invalid)
}
struct Source<'a> {
    segments: &'a Value,
    frames: u64,
    serialized_bytes: usize,
}

pub(super) fn expand_source_binding(
    binding: &Value,
    fixed_voices: &Value,
) -> Result<Value, EditorError> {
    let fields = object(binding, &["ordinarySources", "sourceFormats"])?;
    let voices = bounded_array(fixed_voices, 256)?;
    let formats = bounded_array(&fields["sourceFormats"], 256)?;
    let sources = bounded_array(&fields["ordinarySources"], 32)?;
    // Reject duplicate participants before any dictionary can overwrite them.
    if voices.len() != formats.len()
        || ids(fixed_voices, "elementId")? != ids(&fields["sourceFormats"], "elementId")?
    {
        return Err(invalid());
    }
    let mut assets = BTreeSet::new();
    for voice in voices {
        let value = object(voice, &["elementId", "assetId", "sourceMode"])?;
        identifier(&value["elementId"])?;
        assets.insert(identifier(&value["assetId"])?);
        if !matches!(
            value["sourceMode"].as_str(),
            Some("preserve-pts" | "prepared-concat")
        ) {
            return Err(invalid());
        }
    }
    if assets.len() > 32 {
        return Err(invalid());
    }
    let mut dictionary = BTreeMap::new();
    for source in sources {
        let value = object(source, &["assetId", "segments"])?;
        let asset = identifier(&value["assetId"])?;
        let segments = bounded_array(&value["segments"], 65_536)?;
        if dictionary.contains_key(asset) {
            return Err(invalid());
        }
        // Existing typed segments enforce unknown-field rejection and canonical
        // decimal frame counts. Original JSON PTS and array order are retained.
        let typed: Vec<PreviewAudioSegment> =
            serde_json::from_value(value["segments"].clone()).map_err(|_| invalid())?;
        let mut seen = BTreeSet::new();
        let mut frames = 0u64;
        for segment in typed {
            if segment.frame_count == 0
                || !segment.timestamp_seconds.is_finite()
                || !seen.insert(segment.segment_id)
            {
                return Err(invalid());
            }
            frames = frames
                .checked_add(segment.frame_count)
                .ok_or_else(invalid)?;
        }
        dictionary.insert(
            asset,
            Source {
                segments: &value["segments"],
                frames,
                serialized_bytes: serde_json::to_vec(&value["segments"])
                    .map_err(|_| invalid())?
                    .len(),
            },
        );
    }
    let mut used = BTreeSet::new();
    let mut headers = Vec::with_capacity(formats.len());
    // Array brackets and inter-item commas; account expanded size before any
    // shared segment list is cloned per voice. Small input must not amplify
    // into unbounded native memory or evade the existing facts reply budget.
    let mut bytes = 2usize + formats.len().saturating_sub(1);
    for (format, voice) in formats.iter().zip(voices) {
        let value = object(format, FORMAT_FIELDS)?;
        let mut header = value.clone();
        header.remove("sourceRef");
        header.insert("segments".into(), Value::Array(vec![]));
        let header = Value::Object(header);
        let typed: PreviewSourceFormat =
            serde_json::from_value(header.clone()).map_err(|_| invalid())?;
        if typed.sample_rate == 0
            || typed.sample_rate > 384_000
            || (typed.prepared && typed.frame_count == 0)
            || !(1..=2).contains(&typed.channels)
        {
            return Err(invalid());
        }
        // frameCount remains its canonical decimal spelling, not a JS number.
        decimal(value["frameCount"].as_str().ok_or_else(invalid)?).map_err(|_| invalid())?;
        let source = if typed.prepared {
            if !value["sourceRef"].is_null() || voice["sourceMode"] != "prepared-concat" {
                return Err(invalid());
            }
            None
        } else {
            let reference = identifier(&value["sourceRef"])?;
            if voice["sourceMode"] != "preserve-pts" || voice["assetId"] != reference {
                return Err(invalid());
            }
            let source = dictionary.get(reference).ok_or_else(invalid)?;
            if source.frames != typed.frame_count {
                return Err(invalid());
            }
            used.insert(reference);
            Some(source)
        };
        let header_bytes = serde_json::to_vec(&header).map_err(|_| invalid())?.len();
        let item_bytes = header_bytes
            .checked_sub(2)
            .and_then(|base| base.checked_add(source.map_or(2, |s| s.serialized_bytes)))
            .ok_or_else(budget)?;
        bytes = bytes
            .checked_add(item_bytes)
            .filter(|count| *count <= MAX_EXPANDED_BYTES)
            .ok_or_else(budget)?;
        headers.push((header, source));
    }
    if used.len() != dictionary.len() {
        return Err(invalid());
    }
    Ok(Value::Array(
        headers
            .into_iter()
            .map(|(mut header, source)| {
                if let Some(source) = source {
                    header["segments"] = source.segments.clone();
                }
                header
            })
            .collect(),
    ))
}

#[cfg(test)]
#[path = "shared_binding_tests.rs"]
mod tests;
