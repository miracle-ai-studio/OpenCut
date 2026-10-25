//! Lossless private callback encoding shared by the audio and visual sessions.
//! This is portable protocol data, with no native handles or I/O.
pub(crate) fn bytes_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut hex = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        hex.push(DIGITS[(byte >> 4) as usize] as char);
        hex.push(DIGITS[(byte & 15) as usize] as char);
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::bytes_hex;

    #[test]
    fn callback_bytes_preserve_binary_values_and_chunk_edges() {
        assert_eq!(bytes_hex(&[]), "");
        assert_eq!(bytes_hex(b"\0\x01\x0f\x10\x7f\x80\xff"), "00010f107f80ff");
        // Every binary value must survive the receiver's independent radix decoder.
        let input: Vec<u8> = (0..65_536).map(|n| (n % 256) as u8).collect();
        let encoded = bytes_hex(&input);
        let decoded: Vec<u8> = (0..encoded.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&encoded[i..i + 2], 16).unwrap())
            .collect();
        assert_eq!(decoded, input);
        assert_eq!(encoded.len(), 131_072);
    }
}
