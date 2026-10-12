//! Bounded private NDJSON framing. Domain facts remain typed in application.
use opencut_contracts::EditorError;
use serde_json::{Value, json};
use std::io::Write;
pub(crate) const PACKET_BYTES: usize = 1024 * 1024;
const REPLY_BYTES: usize = 16 * 1024 * 1024;

pub(crate) fn error(code: &str, message: &str) -> EditorError {
    EditorError::new(code, message)
}
pub(crate) fn string<'a>(v: &'a Value, key: &str) -> Result<&'a str, EditorError> {
    v.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| error("AUDIO_PROTOCOL_ERROR", "Missing audio text field"))
}
pub(crate) fn number(v: &Value, key: &str) -> Result<u64, EditorError> {
    v.get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| error("AUDIO_PROTOCOL_ERROR", "Missing audio integer field"))
}
pub(crate) fn packet(
    out: &mut impl Write,
    execution: &str,
    mut value: Value,
) -> Result<(), EditorError> {
    value["protocolVersion"] = json!(1);
    value["executionId"] = json!(execution);
    let bytes = serde_json::to_vec(&value)
        .map_err(|_| error("AUDIO_PROTOCOL_ERROR", "Could not serialize audio packet"))?;
    if bytes.len() > PACKET_BYTES {
        return Err(error(
            "AUDIO_PROTOCOL_ERROR",
            "Audio packet exceeds transport budget",
        ));
    }
    out.write_all(&bytes)
        .and_then(|_| out.write_all(b"\n"))
        .and_then(|_| out.flush())
        .map_err(|_| error("AUDIO_HOST_FAILED", "Audio runner input closed"))
}
pub(crate) fn reply(
    out: &mut impl Write,
    execution: &str,
    id: &str,
    value: Value,
) -> Result<(), EditorError> {
    let bytes = serde_json::to_vec(&value)
        .map_err(|_| error("AUDIO_PROTOCOL_ERROR", "Could not encode audio callback"))?;
    reply_encoded(out, execution, id, &bytes)
}

/// Already encoded, trusted callback facts; framing and all byte admission
/// remain identical to the Value-based callback path.
pub(crate) fn reply_encoded(
    out: &mut impl Write,
    execution: &str,
    id: &str,
    bytes: &[u8],
) -> Result<(), EditorError> {
    if bytes.len() <= PACKET_BYTES - 256 {
        // The payload is already encoded. Preserve the same JSON envelope
        // without rebuilding its dense Value tree and serializing it again.
        let mut prefix = serde_json::to_vec(&json!({"type":"reply","id":id,"ok":true,
            "protocolVersion":1,"executionId":execution}))
        .map_err(|_| error("AUDIO_PROTOCOL_ERROR", "Could not encode audio reply"))?;
        prefix.pop();
        prefix.extend_from_slice(b",\"value\":");
        if prefix.len().saturating_add(bytes.len()).saturating_add(1) > PACKET_BYTES {
            return Err(error(
                "AUDIO_PROTOCOL_ERROR",
                "Audio packet exceeds transport budget",
            ));
        }
        return out
            .write_all(&prefix)
            .and_then(|_| out.write_all(bytes))
            .and_then(|_| out.write_all(b"}\n"))
            .and_then(|_| out.flush())
            .map_err(|_| error("AUDIO_HOST_FAILED", "Audio runner input closed"));
    }
    if bytes.len() > REPLY_BYTES {
        return Err(error(
            "AUDIO_BUDGET_EXCEEDED",
            "Audio callback exceeded bounded reply size",
        ));
    }
    for (sequence, chunk) in bytes.chunks(65_536).enumerate() {
        let hex: String = chunk.iter().map(|b| format!("{b:02x}")).collect();
        packet(
            out,
            execution,
            json!({"type":"reply_chunk","id":id,"sequence":sequence,"bytesHex":hex,"final":(sequence+1)*65_536>=bytes.len()}),
        )?;
    }
    Ok(())
}

pub(crate) fn decode_hex(value: &str) -> Result<Vec<u8>, EditorError> {
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let part = std::str::from_utf8(pair)
                .map_err(|_| error("AUDIO_PROTOCOL_ERROR", "Invalid hex bytes"))?;
            u8::from_str_radix(part, 16)
                .map_err(|_| error("AUDIO_PROTOCOL_ERROR", "Invalid hex bytes"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoded_reply_preserves_escaped_identity_and_nested_facts() {
        let value = json!({"positions":[null,[7,"9007199254740993","9007199254740994",0.25]],
            "amplitudes":[0.000123456789012345],"text":"quote\"\n雪"});
        let mut output = Vec::new();
        reply(&mut output, "execution\"\n", "9", value.clone()).unwrap();
        assert_eq!(output.last(), Some(&b'\n'));
        assert_eq!(output.iter().filter(|&&b| b == b'\n').count(), 1);
        let observed: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(
            observed,
            json!({"type":"reply","id":"9","ok":true,"value":value,
            "protocolVersion":1,"executionId":"execution\"\n"})
        );
    }

    #[test]
    fn large_reply_keeps_bounded_ordered_chunks_and_complete_value() {
        let value = json!({"facts":"x".repeat(PACKET_BYTES)});
        let mut output = Vec::new();
        reply(&mut output, "execution", "10", value.clone()).unwrap();
        let lines: Vec<_> = output
            .split(|&b| b == b'\n')
            .filter(|line| !line.is_empty())
            .collect();
        let mut assembled = Vec::new();
        for (sequence, line) in lines.iter().enumerate() {
            assert!(line.len() <= PACKET_BYTES);
            let packet: Value = serde_json::from_slice(line).unwrap();
            assert_eq!(packet["type"], "reply_chunk");
            assert_eq!(packet["id"], "10");
            assert_eq!(packet["protocolVersion"], 1);
            assert_eq!(packet["executionId"], "execution");
            assert_eq!(packet["sequence"], sequence);
            assert_eq!(packet["final"], sequence + 1 == lines.len());
            let bytes = decode_hex(packet["bytesHex"].as_str().unwrap()).unwrap();
            assert!(bytes.len() <= 65_536);
            assembled.extend(bytes);
        }
        assert_eq!(serde_json::from_slice::<Value>(&assembled).unwrap(), value);
    }
}
