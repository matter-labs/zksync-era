use std::{
    borrow::Cow,
    collections::{hash_map, HashMap},
    fmt,
};

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use zksync_basic_types::{bytecode::validate_bytecode, web3::Bytes, H256, U256};

use crate::Address;

/// Collection of overridden accounts.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StateOverride(HashMap<Address, OverrideAccount>);

impl StateOverride {
    /// Wraps the provided account overrides.
    pub fn new(state: HashMap<Address, OverrideAccount>) -> Self {
        Self(state)
    }

    /// Gets overrides for the specified account.
    pub fn get(&self, address: &Address) -> Option<&OverrideAccount> {
        self.0.get(address)
    }

    /// Gets mutable overrides for the specified account.
    pub fn get_mut(&mut self, address: &Address) -> Option<&mut OverrideAccount> {
        self.0.get_mut(address)
    }

    /// Iterates over all account overrides.
    pub fn iter(&self) -> impl Iterator<Item = (&Address, &OverrideAccount)> + '_ {
        self.0.iter()
    }
}

impl IntoIterator for StateOverride {
    type Item = (Address, OverrideAccount);
    type IntoIter = hash_map::IntoIter<Address, OverrideAccount>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(test, derive(PartialEq))]
pub enum BytecodeOverride {
    EraVm(Bytes),
    Evm(Bytes),
    Unspecified(Bytes),
}

impl AsRef<[u8]> for BytecodeOverride {
    fn as_ref(&self) -> &[u8] {
        let bytes = match self {
            Self::EraVm(bytes) | Self::Evm(bytes) | Self::Unspecified(bytes) => bytes,
        };
        &bytes.0
    }
}

// We need a separate type (vs using `#[serde(untagged)]` on the `Unspecified` variant) to make error messages more comprehensive.
#[derive(Debug, Serialize, Deserialize)]
enum SerdeBytecodeOverride<'a> {
    #[serde(rename = "eravm")]
    EraVm(#[serde(deserialize_with = "deserialize_eravm_bytecode")] Cow<'a, Bytes>),
    #[serde(rename = "evm")]
    Evm(Cow<'a, Bytes>),
}

fn deserialize_eravm_bytecode<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Cow<'static, Bytes>, D::Error> {
    let raw_bytecode = Bytes::deserialize(deserializer)?;
    if let Err(err) = validate_bytecode(&raw_bytecode.0) {
        return Err(de::Error::custom(format!("invalid EraVM bytecode: {err}")));
    }
    Ok(Cow::Owned(raw_bytecode))
}

impl Serialize for BytecodeOverride {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let serde = match self {
            Self::Unspecified(bytes) => return bytes.serialize(serializer),
            Self::Evm(bytes) => SerdeBytecodeOverride::Evm(Cow::Borrowed(bytes)),
            Self::EraVm(bytes) => SerdeBytecodeOverride::EraVm(Cow::Borrowed(bytes)),
        };
        serde.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for BytecodeOverride {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // **Important:** This visitor only works for human-readable deserializers (e.g., JSON).
        struct BytesOrMapVisitor;

        impl<'v> de::Visitor<'v> for BytesOrMapVisitor {
            type Value = BytecodeOverride;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter
                    .write_str("possibly tagged hex string, like \"0x00\" or { \"evm\": \"0xfe\" }")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<BytecodeOverride, E> {
                let deserializer = de::value::StrDeserializer::new(value);
                Bytes::deserialize(deserializer).map(BytecodeOverride::Unspecified)
            }

            fn visit_map<A: de::MapAccess<'v>>(self, data: A) -> Result<Self::Value, A::Error> {
                let deserializer = de::value::MapAccessDeserializer::new(data);
                Ok(match SerdeBytecodeOverride::deserialize(deserializer)? {
                    SerdeBytecodeOverride::Evm(bytes) => BytecodeOverride::Evm(bytes.into_owned()),
                    SerdeBytecodeOverride::EraVm(bytes) => {
                        BytecodeOverride::EraVm(bytes.into_owned())
                    }
                })
            }
        }

        deserializer.deserialize_any(BytesOrMapVisitor)
    }
}

/// Account override for `eth_call`, `eth_estimateGas` and other VM-invoking methods.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
#[serde(rename_all = "camelCase")]
pub struct OverrideAccount {
    pub balance: Option<U256>,
    pub nonce: Option<U256>,
    pub code: Option<BytecodeOverride>,
    #[serde(flatten, deserialize_with = "state_deserializer")]
    pub state: Option<OverrideState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
#[serde(rename_all = "camelCase")]
pub enum OverrideState {
    State(HashMap<H256, H256>),
    StateDiff(HashMap<H256, H256>),
}

fn state_deserializer<'de, D>(deserializer: D) -> Result<Option<OverrideState>, D::Error>
where
    D: Deserializer<'de>,
{
    let val = serde_json::Value::deserialize(deserializer)?;
    let state: Option<HashMap<H256, H256>> = match val.get("state") {
        Some(val) => serde_json::from_value(val.clone()).map_err(de::Error::custom)?,
        None => None,
    };
    let state_diff: Option<HashMap<H256, H256>> = match val.get("stateDiff") {
        Some(val) => serde_json::from_value(val.clone()).map_err(de::Error::custom)?,
        None => None,
    };

    match (state, state_diff) {
        (Some(state), None) => Ok(Some(OverrideState::State(state))),
        (None, Some(state_diff)) => Ok(Some(OverrideState::StateDiff(state_diff))),
        (None, None) => Ok(None),
        _ => Err(de::Error::custom(
            "Both 'state' and 'stateDiff' cannot be set simultaneously",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializing_bytecode_override() {
        let json = serde_json::json!("0x00");
        let bytecode: BytecodeOverride = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(bytecode, BytecodeOverride::Unspecified(Bytes(vec![0])));
        assert_eq!(serde_json::to_value(&bytecode).unwrap(), json);

        let json = serde_json::json!({ "evm": "0xfe" });
        let bytecode: BytecodeOverride = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(bytecode, BytecodeOverride::Evm(Bytes(vec![0xfe])));
        assert_eq!(serde_json::to_value(&bytecode).unwrap(), json);

        let json = serde_json::json!({ "eravm": "0xfe" });
        let err = serde_json::from_value::<BytecodeOverride>(json)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("invalid EraVM bytecode") && err.contains("not divisible by 32"),
            "{err}"
        );

        let json = serde_json::json!({ "what": "0xfe" });
        let err = serde_json::from_value::<BytecodeOverride>(json)
            .unwrap_err()
            .to_string();
        assert!(err.contains("unknown variant"), "{err}");

        let json = serde_json::json!("what");
        let err = serde_json::from_value::<BytecodeOverride>(json)
            .unwrap_err()
            .to_string();
        assert!(err.contains("expected 0x prefix"));
    }

    #[test]
    fn deserializing_state_override() {
        let json = serde_json::json!({
            "0x0123456789abcdef0123456789abcdef01234567": {
                "balance": "0x123",
                "nonce": "0x1",
                "code": "0x00",
            },
            "0x123456789abcdef0123456789abcdef012345678": {
                "stateDiff": {
                    "0x0000000000000000000000000000000000000000000000000000000000000000":
                        "0x0000000000000000000000000000000000000000000000000000000000000001",
                    "0x0000000000000000000000000000000000000000000000000000000000000001":
                        "0x0000000000000000000000000000000000000000000000000000000000000002",
                },
                "code": { "evm": "0xfe" },
            }
        });

        let state_override: StateOverride = serde_json::from_value(json).unwrap();
        assert_eq!(state_override.0.len(), 2);

        let first_address: Address = "0x0123456789abcdef0123456789abcdef01234567"
            .parse()
            .unwrap();
        let first_override = &state_override.0[&first_address];
        assert_eq!(
            *first_override,
            OverrideAccount {
                balance: Some(0x123.into()),
                nonce: Some(1.into()),
                code: Some(BytecodeOverride::Unspecified(Bytes(vec![0]))),
                ..OverrideAccount::default()
            }
        );

        let second_address: Address = "0x123456789abcdef0123456789abcdef012345678"
            .parse()
            .unwrap();
        let second_override = &state_override.0[&second_address];
        assert_eq!(
            *second_override,
            OverrideAccount {
                state: Some(OverrideState::StateDiff(HashMap::from([
                    (H256::from_low_u64_be(0), H256::from_low_u64_be(1)),
                    (H256::from_low_u64_be(1), H256::from_low_u64_be(2)),
                ]))),
                code: Some(BytecodeOverride::Evm(Bytes(vec![0xfe]))),
                ..OverrideAccount::default()
            }
        );
    }

    #[test]
    fn deserializing_bogus_account_override() {
        let json = serde_json::json!({
            "state": {
                "0x0000000000000000000000000000000000000000000000000000000000000001":
                    "0x0000000000000000000000000000000000000000000000000000000000000002",
            },
            "stateDiff": {
                "0x0000000000000000000000000000000000000000000000000000000000000000":
                    "0x0000000000000000000000000000000000000000000000000000000000000001",
            },
        });
        let err = serde_json::from_value::<OverrideAccount>(json).unwrap_err();
        assert!(err.to_string().contains("'state' and 'stateDiff'"), "{err}");
    }
}
