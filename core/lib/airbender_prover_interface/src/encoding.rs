use crate::inputs::AirbenderVerifierInput;

/// Encodes an `AirbenderVerifierInput` into a hex string of packed 32-bit words.
///
/// The format is: first word is the byte length, followed by the bincode-serialized
/// data packed into big-endian u32 words, all rendered as contiguous hex.
pub fn encode_input_to_hex(input: &AirbenderVerifierInput) -> Result<String, String> {
    let encoded = bincode::serialize(input)
        .map_err(|err| format!("Failed to serialize verifier input: {err}"))?;

    let words = encode_to_words(&encoded)?;

    let mut hex = String::with_capacity(words.len() * 8);
    for word in words {
        use std::fmt::Write;
        write!(hex, "{:08x}", word).map_err(|err| format!("Failed to write hex word: {err}"))?;
    }

    Ok(hex)
}

/// Packs a byte slice into a length-prefixed sequence of big-endian u32 words.
pub fn encode_to_words(bytes: &[u8]) -> Result<Vec<u32>, String> {
    if bytes.len() > u32::MAX as usize {
        return Err("Encoded input is larger than u32::MAX bytes".to_string());
    }

    let mut words = Vec::with_capacity(1 + bytes.len().div_ceil(4));
    words.push(bytes.len() as u32);

    for chunk in bytes.chunks(4) {
        let mut buf = [0u8; 4];
        buf[..chunk.len()].copy_from_slice(chunk);
        words.push(u32::from_be_bytes(buf));
    }

    Ok(words)
}

/// Unpacks a length-prefixed sequence of big-endian u32 words back into bytes.
pub fn decode_from_words(words: &[u32]) -> Result<Vec<u8>, String> {
    if words.is_empty() {
        return Err("No words provided".to_string());
    }

    let byte_len = words[0] as usize;
    let available = words.len().saturating_sub(1) * 4;
    if byte_len > available {
        return Err(format!(
            "Declared length {} exceeds available bytes {}",
            byte_len, available
        ));
    }

    let mut bytes = Vec::with_capacity(byte_len);
    for word in &words[1..] {
        bytes.extend_from_slice(&word.to_be_bytes());
    }
    bytes.truncate(byte_len);
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::{decode_from_words, encode_to_words};

    #[test]
    fn encode_decode_roundtrip_with_padding() {
        let input = vec![0x01, 0x02, 0x03, 0x04, 0x05];
        let words = encode_to_words(&input).expect("encode");
        assert_eq!(words[0], 5);
        assert_eq!(words.len(), 3);
        assert_eq!(words[1], 0x01020304);
        assert_eq!(words[2], 0x05000000);

        let decoded = decode_from_words(&words).expect("decode");
        assert_eq!(decoded, input);
    }

    #[test]
    fn encode_decode_roundtrip_empty() {
        let input = Vec::<u8>::new();
        let words = encode_to_words(&input).expect("encode");
        assert_eq!(words, vec![0]);
        let decoded = decode_from_words(&words).expect("decode");
        assert!(decoded.is_empty());
    }
}
