use std::{collections::HashMap, ops::Deref};

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use zksync_basic_types::{web3::Bytes, H256, U256};
use zksync_utils::bytecode::{hash_bytecode, validate_bytecode, InvalidBytecodeError};

use crate::Address;

/// Collection of overridden accounts.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StateOverride(HashMap<Address, OverrideAccount>);

/// Serialized bytecode representation.
#[derive(Debug, Clone, PartialEq)]
pub struct Bytecode(Bytes);

impl Bytecode {
    pub fn new(bytes: Vec<u8>) -> Result<Self, InvalidBytecodeError> {
        validate_bytecode(&bytes)?;
        Ok(Self(Bytes(bytes)))
    }

    /// Returns the canonical hash of this bytecode.
    pub fn hash(&self) -> H256 {
        hash_bytecode(&self.0 .0)
    }

    /// Converts this bytecode into bytes.
    pub fn into_bytes(self) -> Vec<u8> {
        self.0 .0
    }
}

impl Serialize for Bytecode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Bytecode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let bytes = Bytes::deserialize(deserializer)?;
        validate_bytecode(&bytes.0).map_err(de::Error::custom)?;
        Ok(Self(bytes))
    }
}

/// Account override for `eth_estimateGas`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
#[serde(rename_all = "camelCase")]
pub struct OverrideAccount {
    pub balance: Option<U256>,
    pub nonce: Option<U256>,
    pub code: Option<Bytecode>,
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

impl StateOverride {
    pub fn new(state: HashMap<Address, OverrideAccount>) -> Self {
        Self(state)
    }

    pub fn get(&self, address: &Address) -> Option<&OverrideAccount> {
        self.0.get(address)
    }
}

// FIXME: remove
impl Deref for StateOverride {
    type Target = HashMap<Address, OverrideAccount>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializing_bytecode() {
        let bytecode_str = "0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let json = serde_json::Value::String(bytecode_str.to_owned());
        let bytecode: Bytecode = serde_json::from_value(json).unwrap();
        assert_ne!(bytecode.hash(), H256::zero());
        let bytecode = bytecode.into_bytes();
        assert_eq!(bytecode.len(), 32);
        assert_eq!(bytecode[0], 0x01);
        assert_eq!(bytecode[31], 0xef);
    }

    #[test]
    fn deserializing_invalid_bytecode() {
        let invalid_bytecodes = [
            "1234",   // not 0x-prefixed
            "0x1234", // length not divisible by 32
            "0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\
             0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef", // even number of words
        ];
        for bytecode_str in invalid_bytecodes {
            let json = serde_json::Value::String(bytecode_str.to_owned());
            serde_json::from_value::<Bytecode>(json).unwrap_err();
        }

        let long_bytecode = String::from("0x")
            + &"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".repeat(65_537);
        let json = serde_json::Value::String(long_bytecode);
        serde_json::from_value::<Bytecode>(json).unwrap_err();
    }

    #[test]
    fn deserializing_state_override() {
        let json = serde_json::json!({
            "0x0123456789abcdef0123456789abcdef01234567": {
                "balance": "0x123",
                "nonce": "0x1",
            },
            "0x123456789abcdef0123456789abcdef012345678": {
                "stateDiff": {
                    "0x0000000000000000000000000000000000000000000000000000000000000000":
                        "0x0000000000000000000000000000000000000000000000000000000000000001",
                    "0x0000000000000000000000000000000000000000000000000000000000000001":
                        "0x0000000000000000000000000000000000000000000000000000000000000002",
                }
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
