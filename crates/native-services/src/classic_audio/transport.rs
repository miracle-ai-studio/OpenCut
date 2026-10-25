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
    value["executionId"] = json!(execution);
    let bytes = serde_json::to_vec(&value)
        .map_err(|_| error("AUDIO_PROTOCOL_ERROR", "Could not serialize audio packet"))?;
    write_packet(out, &bytes)
}
fn write_packet(out: &mut impl Write, bytes: &[u8]) -> Result<(), EditorError> {
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
            "executionId":execution}))
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
    // Keep the same decoded budget while avoiding a large JSON metadata stream
    // crossing the local pipe twice as hex. Incompressible facts keep raw framing.
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder
        .write_all(bytes)
        .map_err(|_| error("AUDIO_PROTOCOL_ERROR", "Could not compress audio facts"))?;
    let compressed = encoder
        .finish()
        .map_err(|_| error("AUDIO_PROTOCOL_ERROR", "Could not finish audio facts"))?;
    let encoded = if compressed.len() < bytes.len() {
        compressed.as_slice()
    } else {
        bytes
    };
    let compressed = encoded.len() < bytes.len();
    // Escape identities once. Hex is generated here and contains only ASCII
    // digits, so it needs no second JSON string scan for every bounded chunk.
    let execution_json = serde_json::to_vec(execution).map_err(|_| {
        error(
            "AUDIO_PROTOCOL_ERROR",
            "Could not encode execution identity",
        )
    })?;
    let id_json = serde_json::to_vec(id)
        .map_err(|_| error("AUDIO_PROTOCOL_ERROR", "Could not encode callback identity"))?;
    for (sequence, chunk) in encoded.chunks(65_536).enumerate() {
        let hex = encode_hex(chunk);
        let mut packet = Vec::with_capacity(hex.len() + execution_json.len() + id_json.len() + 96);
        packet.extend_from_slice(b"{\"bytesHex\":\"");
        packet.extend_from_slice(hex.as_bytes());
        packet.extend_from_slice(b"\"");
        if compressed {
            packet.extend_from_slice(b",\"decodedBytes\":");
            packet.extend_from_slice(bytes.len().to_string().as_bytes());
            packet.extend_from_slice(b",\"encoding\":\"zlib\"");
        }
        packet.extend_from_slice(b",\"executionId\":");
        packet.extend_from_slice(&execution_json);
        packet.extend_from_slice(b",\"final\":");
        packet.extend_from_slice(if (sequence + 1) * 65_536 >= encoded.len() {
            b"true"
        } else {
            b"false"
        });
        packet.extend_from_slice(b",\"id\":");
        packet.extend_from_slice(&id_json);
        packet.extend_from_slice(b",\"sequence\":");
        packet.extend_from_slice(sequence.to_string().as_bytes());
        packet.extend_from_slice(b",\"type\":\"reply_chunk\"}");
        write_packet(out, &packet)?;
    }
    Ok(())
}

fn encode_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        encoded.push(DIGITS[(byte >> 4) as usize] as char);
        encoded.push(DIGITS[(byte & 15) as usize] as char);
    }
    encoded
}

pub(crate) fn decode_hex(value: &str) -> Result<Vec<u8>, EditorError> {
    fn nibble(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        }
    }
    let invalid = || error("AUDIO_PROTOCOL_ERROR", "Invalid hex bytes");
    if value.len() % 2 != 0 {
        return Err(invalid());
    }
    // Audio and video share this bounded transport. Decode ASCII directly;
    // parsing a UTF-8 string and integer for every RGBA byte dominates chunks.
    let mut bytes = Vec::with_capacity(value.len() / 2);
    for pair in value.as_bytes().chunks_exact(2) {
        let high = nibble(pair[0]).ok_or_else(invalid)?;
        let low = nibble(pair[1]).ok_or_else(invalid)?;
        bytes.push((high << 4) | low);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_transport_roundtrips_every_byte_and_rejects_partial_or_non_hex_input() {
        let expected: Vec<u8> = (0..=255).collect();
        let hex = encode_hex(&expected);
        assert_eq!(encode_hex(&[0, 15, 16, 127, 128, 255]), "000f107f80ff");
        assert_eq!(encode_hex(&[]), "");
        assert_eq!(decode_hex(&hex).unwrap(), expected);
        assert_eq!(decode_hex(&hex.to_uppercase()).unwrap(), expected);
        assert!(decode_hex("").unwrap().is_empty());
        for invalid in ["0", "000", "0g", "+f", " f", "雪", "é"] {
            assert_eq!(
                decode_hex(invalid).unwrap_err().code,
                "AUDIO_PROTOCOL_ERROR"
            );
        }
    }

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
            "executionId":"execution\"\n"})
        );
    }

    #[test]
    fn large_reply_keeps_bounded_ordered_chunks_and_complete_value() {
        let mut state = 0x12345678u32;
        let facts: String = (0..PACKET_BYTES)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                (32 + state % 95) as u8 as char
            })
            .collect();
        let value = json!({"facts":facts, "text":"quote\"\n雪"});
        let mut output = Vec::new();
        reply(&mut output, "execution\"\n雪", "10\"", value.clone()).unwrap();
        let lines: Vec<_> = output
            .split(|&b| b == b'\n')
            .filter(|line| !line.is_empty())
            .collect();
        assert!(lines.len() > 1);
        let mut assembled = Vec::new();
        for (sequence, line) in lines.iter().enumerate() {
            assert!(line.len() <= PACKET_BYTES);
            let packet: Value = serde_json::from_slice(line).unwrap();
            assert_eq!(packet["type"], "reply_chunk");
            assert_eq!(packet["id"], "10\"");
            assert!(packet.get("protocolVersion").is_none());
            assert_eq!(packet["executionId"], "execution\"\n雪");
            assert_eq!(packet["sequence"], sequence);
            assert_eq!(packet["final"], sequence + 1 == lines.len());
            assert_eq!(packet["encoding"], "zlib");
            assert_eq!(
                packet["decodedBytes"],
                serde_json::to_vec(&value).unwrap().len()
            );
            let bytes = decode_hex(packet["bytesHex"].as_str().unwrap()).unwrap();
            assert!(bytes.len() <= 65_536);
            assembled.extend(bytes);
        }
        let mut inflated = Vec::new();
        std::io::Read::read_to_end(
            &mut flate2::read::ZlibDecoder::new(assembled.as_slice()),
            &mut inflated,
        )
        .unwrap();
        let assembled = inflated;
        assert_eq!(serde_json::from_slice::<Value>(&assembled).unwrap(), value);
        assert_eq!(assembled, serde_json::to_vec(&value).unwrap());
    }
}
