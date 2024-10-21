use std::{fmt, fmt::Display, marker::PhantomData, str::FromStr, time::Duration};

use serde::{de, Deserialize};
use zksync_basic_types::L1BatchNumber;

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct TeeConfig {
    /// If true, the TEE support is enabled.
    #[serde(deserialize_with = "deserialize_stringified_any")]
    pub tee_support: bool,
    /// All batches before this one are considered to be processed.
    #[serde(deserialize_with = "deserialize_stringified_any", default)]
    pub first_tee_processed_batch: L1BatchNumber,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ProofDataHandlerConfig {
    pub http_port: u16,
    pub proof_generation_timeout_in_secs: u16,
    #[serde(flatten)]
    pub tee_config: TeeConfig,
}

impl ProofDataHandlerConfig {
    pub fn proof_generation_timeout(&self) -> Duration {
        Duration::from_secs(self.proof_generation_timeout_in_secs as u64)
    }
}

// Boilerplate to workaround https://github.com/softprops/envy/issues/26

pub fn deserialize_stringified_any<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: de::Deserializer<'de>,
    T: FromStr,
    T::Err: Display,
{
    deserializer.deserialize_any(StringifiedAnyVisitor(PhantomData))
}

pub struct StringifiedAnyVisitor<T>(PhantomData<T>);

impl<'de, T> de::Visitor<'de> for StringifiedAnyVisitor<T>
where
    T: FromStr,
    T::Err: Display,
{
    type Value = T;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a string containing json data")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Self::Value::from_str(v).map_err(E::custom)
    }
}
