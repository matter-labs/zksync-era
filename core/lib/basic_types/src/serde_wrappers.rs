//! Generic `serde` helpers.

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

/// Trait for specifying prefix for bytes to hex serialization
pub trait Prefix {
    fn prefix() -> &'static str;
}

/// "0x" hex prefix
pub struct ZeroxPrefix;
impl Prefix for ZeroxPrefix {
    fn prefix() -> &'static str {
        "0x"
    }
}

/// `BytesToHexSerde` is a helper struct that allows to serialize and deserialize `Vec<u8>` fields as hex-encoded strings with a prefix.
///
/// Used to annotate `Vec<u8>` fields that you want to serialize like hex-encoded string with prefix
/// Use this struct in annotation like that `[serde(with = "BytesToHexSerde::<T>"]`
/// where T is concrete prefix type (e.g. `SyncBlockPrefix`)
pub struct BytesToHexSerde<P> {
    _marker: std::marker::PhantomData<P>,
}

impl<P: Prefix> BytesToHexSerde<P> {
    pub fn serialize<S>(value: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            // First, serialize to hexadecimal string.
            let hex_value = format!("{}{}", P::prefix(), hex::encode(value));

            // Then, serialize it using `Serialize` trait implementation for `String`.
            String::serialize(&hex_value, serializer)
        } else {
            <[u8]>::serialize(value, serializer)
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            let deserialized_string = String::deserialize(deserializer)?;

            if let Some(deserialized_string) = deserialized_string.strip_prefix(P::prefix()) {
                hex::decode(deserialized_string).map_err(de::Error::custom)
            } else {
                Err(de::Error::custom(format!(
                    "string value missing prefix: {:?}",
                    P::prefix()
                )))
            }
        } else {
            <Vec<u8>>::deserialize(deserializer)
        }
    }
}

pub type ZeroPrefixHexSerde = BytesToHexSerde<ZeroxPrefix>;

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Execute {
        #[serde(with = "ZeroPrefixHexSerde")]
        pub calldata: Vec<u8>,
    }

    #[test]
    fn test_hex_serde_bincode() {
        let original = Execute {
            calldata: vec![0, 1, 2, 3, 4],
        };
        let encoded: Vec<u8> = vec![5, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4];
        let decoded: Execute = bincode::deserialize(&encoded).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_hex_serde_json() {
        let original = Execute {
            calldata: vec![0, 1, 2, 3, 4],
        };
        let encoded = serde_json::to_string(&original).unwrap();
        assert_eq!(r#"{"calldata":"0x0001020304"}"#, encoded);
        let decoded: Execute = serde_json::from_str(&encoded).unwrap();
        assert_eq!(original, decoded);
    }
}
