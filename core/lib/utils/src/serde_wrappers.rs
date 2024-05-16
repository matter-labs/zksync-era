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
        // First, serialize to hexadecimal string.
        let hex_value = format!("{}{}", P::prefix(), hex::encode(value));

        // Then, serialize it using `Serialize` trait implementation for `String`.
        String::serialize(&hex_value, serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let deserialized_string = String::deserialize(deserializer)?;

        if let Some(deserialized_string) = deserialized_string.strip_prefix(P::prefix()) {
            hex::decode(deserialized_string).map_err(de::Error::custom)
        } else {
            Err(de::Error::custom(format!(
                "string value missing prefix: {:?}",
                P::prefix()
            )))
        }
    }
}

pub type ZeroPrefixHexSerde = BytesToHexSerde<ZeroxPrefix>;
