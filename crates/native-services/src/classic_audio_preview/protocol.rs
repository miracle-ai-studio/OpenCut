use crate::classic_audio::transport::{decode_hex, error, string};
use opencut_contracts::EditorError;
use serde_json::{Value, json};
#[path = "compiled_packet.rs"]
mod compiled_packet;
#[path = "shared_binding.rs"]
mod shared_binding;

pub(super) fn pack_preview_block(
    block: opencut_composition::project_v4::PreviewAudioBlockV4,
) -> Result<Vec<u8>, EditorError> {
    compiled_packet::pack_preview_block(block)
}

pub(super) fn pack_preview_block_v2(
    block: opencut_composition::project_v4::PreviewAudioBlockV4,
) -> Result<Vec<u8>, EditorError> {
    compiled_packet::pack_preview_block_v2(block)
}

pub(super) fn expand_source_binding(
    binding: &Value,
    fixed_voices: &Value,
) -> Result<Value, EditorError> {
    shared_binding::expand_source_binding(binding, fixed_voices)
}

pub(super) fn ids(values: &Value, key: &str) -> Result<Vec<String>, EditorError> {
    // Resources and whole-scene voice metadata have independent bounds. This
    // does not perform the pure composition's concurrent/block admission.
    let maximum = match key {
        "assetId" => 32,
        "elementId" => 256,
        _ => {
            return Err(error(
                "AUDIO_PROTOCOL_ERROR",
                "Unknown preview identity collection",
            ));
        }
    };
    let values = values
        .as_array()
        .filter(|a| a.len() <= maximum)
        .ok_or_else(|| {
            error(
                "AUDIO_PROTOCOL_ERROR",
                "Preview identity collection exceeds its bounds",
            )
        })?;
    let mut ids = Vec::with_capacity(values.len());
    for value in values {
        let id = string(value, key)?;
        if id.is_empty() || ids.iter().any(|prior| prior == id) {
            return Err(error(
                "AUDIO_PROTOCOL_ERROR",
                "Invalid duplicate preview identity",
            ));
        }
        ids.push(id.to_owned());
    }
    Ok(ids)
}
// Number spelling differs between JS JSON and serde (notably small PTS).
// Bind semantic numbers by IEEE-f64 bits, arrays in order, object keys sorted.
pub(super) fn canonical_binding(value: &Value) -> Result<Value, EditorError> {
    Ok(match value {
        Value::Number(n) => {
            let number = n
                .as_f64()
                .filter(|v| v.is_finite())
                .ok_or_else(|| error("AUDIO_PROTOCOL_ERROR", "Invalid binding number"))?;
            json!({"$f64":format!("{:016x}",if number == 0.0 {0} else {number.to_bits()})})
        }
        Value::Array(values) => Value::Array(
            values
                .iter()
                .map(canonical_binding)
                .collect::<Result<_, _>>()?,
        ),
        Value::Object(values) => {
            let mut keys: Vec<_> = values.keys().collect();
            keys.sort();
            let mut result = serde_json::Map::new();
            for key in keys {
                result.insert(key.clone(), canonical_binding(&values[key])?);
            }
            Value::Object(result)
        }
        _ => value.clone(),
    })
}
pub(super) fn decode_pcm(hex: &str, frames: u32) -> Result<Vec<f32>, EditorError> {
    if hex.len() != frames as usize * 8
        || !hex
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(error("AUDIO_PROTOCOL_ERROR", "Invalid preview f32le bytes"));
    }
    decode_hex(hex)?
        .chunks_exact(4)
        .map(|bytes| {
            let value = f32::from_le_bytes(bytes.try_into().expect("Four-byte chunk"));
            if !value.is_finite() {
                return Err(error("INVALID_DECODED_PCM", "Non-finite preview PCM"));
            }
            Ok(value)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_pcm_is_finite_exact_and_credit_bounded() {
        assert_eq!(decode_pcm("0000003f000080be", 2).unwrap(), [0.5, -0.25]);
        assert!(decode_pcm("0000003f", 2).is_err());
        assert!(decode_pcm("0000c07f", 1).is_err());
        assert!(decode_pcm("0000003F", 1).is_err());
    }
    #[test]
    fn preview_binding_numbers_use_f64_bits_with_sorted_keys_and_normalized_zero() {
        let facts = json!({"b":0.000001,"a":-0.0});
        assert_eq!(
            serde_json::to_string(&canonical_binding(&facts).unwrap()).unwrap(),
            r#"{"a":{"$f64":"0000000000000000"},"b":{"$f64":"3eb0c6f7a0b5ed8d"}}"#
        );
    }
    #[test]
    fn resources_and_voice_metadata_have_separate_identity_bounds() {
        let voices = Value::Array(
            (0..256)
                .map(|i| json!({"elementId": format!("e{i}")}))
                .collect(),
        );
        assert_eq!(ids(&voices, "elementId").unwrap().len(), 256);
        let mut extra = voices.clone();
        extra
            .as_array_mut()
            .unwrap()
            .push(json!({"elementId":"extra"}));
        assert!(ids(&extra, "elementId").is_err());
        let resources = Value::Array(
            (0..32)
                .map(|i| json!({"assetId": format!("a{i}")}))
                .collect(),
        );
        assert_eq!(ids(&resources, "assetId").unwrap().len(), 32);
        let mut extra = resources.clone();
        extra
            .as_array_mut()
            .unwrap()
            .push(json!({"assetId":"extra"}));
        assert!(ids(&extra, "assetId").is_err());
        assert!(ids(&json!([{"elementId":"e"},{"elementId":"e"}]), "elementId").is_err());
        assert!(ids(&voices, "unknown").is_err());
    }
}
