use std::fmt;

use ethabi::ethereum_types::U256;
use serde::{de, de::SeqAccess, ser, ser::SerializeSeq, Deserializer, Serializer};

pub(super) fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
    if bytes.len() % 32 != 0 {
        return Err(ser::Error::custom("bytecode length is not divisible by 32"));
    }
    let mut seq = serializer.serialize_seq(Some(bytes.len() % 32))?;
    for chunk in bytes.chunks(32) {
        let word = U256::from_big_endian(chunk);
        seq.serialize_element(&word)?;
    }
    seq.end()
}

#[derive(Debug)]
struct SeqVisitor;

impl<'de> de::Visitor<'de> for SeqVisitor {
    type Value = Vec<u8>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "sequence of `U256` words")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let len = seq.size_hint().unwrap_or(0) * 32;
        let mut bytes = Vec::with_capacity(len);
        while let Some(value) = seq.next_element::<U256>()? {
            let prev_len = bytes.len();
            bytes.resize(prev_len + 32, 0);
            value.to_big_endian(&mut bytes[prev_len..]);
        }
        Ok(bytes)
    }
}

pub(super) fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
    deserializer.deserialize_seq(SeqVisitor)
}

#[cfg(test)]
mod tests {
    use ethabi::ethereum_types::H256;

    use crate::SystemContractCode;

    #[test]
    fn serializing_system_contract_code() {
        let mut code = vec![0; 32];
        code.extend_from_slice(&[0; 30]);
        code.extend_from_slice(&[0xab, 0xcd]);
        code.extend_from_slice(&[0x23; 32]);

        let system_contract_code = SystemContractCode {
            hash: H256::repeat_byte(0x42),
            code,
        };
        let json = serde_json::to_value(&system_contract_code).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "code": ["0x0", "0xabcd", "0x2323232323232323232323232323232323232323232323232323232323232323"],
                "hash": "0x4242424242424242424242424242424242424242424242424242424242424242",
            })
        );

        let restored: SystemContractCode = serde_json::from_value(json).unwrap();
        assert_eq!(restored.code, system_contract_code.code);
        assert_eq!(restored.hash, system_contract_code.hash);
    }
}
