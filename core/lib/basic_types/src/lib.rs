//! The declaration of the most primitive types used in ZKsync network.
//!
//! Most of them are just re-exported from the `web3` crate.

// Linter settings
#![warn(clippy::cast_lossless)]

use std::{
    convert::{Infallible, TryFrom, TryInto},
    fmt,
    num::ParseIntError,
    ops::{Add, Deref, DerefMut, Sub},
    str::FromStr,
};

pub use ethabi::{
    self,
    ethereum_types::{Address, Bloom as H2048, H128, H160, H256, H512, H520, H64, U128, U256, U64},
};
use serde::{de, Deserialize, Deserializer, Serialize};

#[macro_use]
mod macros;
pub mod basic_fri_types;
pub mod commitment;
pub mod network;
pub mod protocol_version;
pub mod prover_dal;
pub mod url;
pub mod vm_version;
pub mod web3;

/// Account place in the global state tree is uniquely identified by its address.
/// Binary this type is represented by 160 bit big-endian representation of account address.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, Hash, Ord, PartialOrd)]
pub struct AccountTreeId {
    address: Address,
}

impl AccountTreeId {
    pub fn new(address: Address) -> Self {
        Self { address }
    }

    pub fn address(&self) -> &Address {
        &self.address
    }

    #[allow(clippy::wrong_self_convention)] // In that case, reference makes more sense.
    pub fn to_fixed_bytes(&self) -> [u8; 20] {
        let mut result = [0u8; 20];
        result.copy_from_slice(&self.address.to_fixed_bytes());
        result
    }

    pub fn from_fixed_bytes(value: [u8; 20]) -> Self {
        let address = Address::from_slice(&value);
        Self { address }
    }
}

impl Default for AccountTreeId {
    fn default() -> Self {
        Self {
            address: Address::zero(),
        }
    }
}

#[allow(clippy::from_over_into)]
impl Into<U256> for AccountTreeId {
    fn into(self) -> U256 {
        let mut be_data = [0u8; 32];
        be_data[12..].copy_from_slice(&self.to_fixed_bytes());
        U256::from_big_endian(&be_data)
    }
}

impl TryFrom<U256> for AccountTreeId {
    type Error = Infallible;

    fn try_from(val: U256) -> Result<Self, Infallible> {
        let mut be_data = vec![0; 32];
        val.to_big_endian(&mut be_data);
        Ok(Self::from_fixed_bytes(be_data[12..].try_into().unwrap()))
    }
}

/// ChainId in the ZKsync network.
#[derive(Copy, Clone, Debug, Serialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct L2ChainId(u64);

impl<'de> Deserialize<'de> for L2ChainId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            let value: serde_json::Value = Deserialize::deserialize(deserializer)?;
            match &value {
                serde_json::Value::Number(number) => Self::new(number.as_u64().ok_or(
                    de::Error::custom(format!("Failed to parse: {}, Expected u64", number)),
                )?)
                .map_err(de::Error::custom),
                serde_json::Value::String(string) => string.parse().map_err(de::Error::custom),
                _ => Err(de::Error::custom(format!(
                    "Failed to parse: {}, Expected number or string",
                    value
                ))),
            }
        } else {
            u64::deserialize(deserializer).map(L2ChainId)
        }
    }
}

impl L2ChainId {
    fn new(number: u64) -> Result<Self, String> {
        if number > L2ChainId::max().0 {
            return Err(format!(
                "Cannot convert given value {} into L2ChainId. It's greater than MAX: {}",
                number,
                L2ChainId::max().0
            ));
        }
        Ok(L2ChainId(number))
    }
}

impl FromStr for L2ChainId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Parse the string as a U64
        // try to parse as decimal first
        let number = match U64::from_dec_str(s) {
            Ok(u) => u,
            Err(_) => {
                // try to parse as hex
                s.parse::<U64>()
                    .map_err(|err| format!("Failed to parse L2ChainId: Err {err}"))?
            }
        };
        L2ChainId::new(number.as_u64())
    }
}

impl L2ChainId {
    /// The maximum value of the L2 chain ID.
    // `2^53 - 1` is a max safe integer in JS. In Ethereum JS libraries chain ID should be the safe integer.
    // Next arithmetic operation: subtract 36 and divide by 2 comes from `v` calculation:
    // `v = 2*chainId + 36`, that should be save integer as well.
    const MAX: u64 = ((1 << 53) - 1 - 36) / 2;

    pub fn max() -> Self {
        Self(Self::MAX)
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl Default for L2ChainId {
    fn default() -> Self {
        Self(270)
    }
}

impl TryFrom<u64> for L2ChainId {
    type Error = String;

    fn try_from(val: u64) -> Result<Self, Self::Error> {
        Self::new(val)
    }
}

impl From<u32> for L2ChainId {
    fn from(value: u32) -> Self {
        // Max value is guaranteed bigger than u32
        Self(u64::from(value))
    }
}

basic_type!(
    /// ZKsync network block sequential index.
    L2BlockNumber,
    u32
);

basic_type!(
    /// ZKsync L1 batch sequential index.
    L1BatchNumber,
    u32
);

basic_type!(
    /// Ethereum network block sequential index.
    L1BlockNumber,
    u32
);

basic_type!(
    /// ZKsync account nonce.
    Nonce,
    u32
);

basic_type!(
    /// Unique identifier of the priority operation in the ZKsync network.
    PriorityOpId,
    u64
);

basic_type!(
    /// ChainId in the Ethereum network.
    L1ChainId,
    u64
);

#[allow(clippy::derivable_impls)]
impl Default for L2BlockNumber {
    fn default() -> Self {
        Self(0)
    }
}

#[allow(clippy::derivable_impls)]
impl Default for L1BatchNumber {
    fn default() -> Self {
        Self(0)
    }
}

#[allow(clippy::derivable_impls)]
impl Default for L1BlockNumber {
    fn default() -> Self {
        Self(0)
    }
}

#[allow(clippy::derivable_impls)]
impl Default for PriorityOpId {
    fn default() -> Self {
        Self(0)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::from_str;

    use super::*;

    #[test]
    fn test_from_str_valid_decimal() {
        let input = "42";
        let result = L2ChainId::from_str(input);
        assert_eq!(result.unwrap().as_u64(), 42);
    }

    #[test]
    fn test_serialize_deserialize() {
        #[derive(Serialize, Deserialize)]
        struct Test {
            chain_id: L2ChainId,
        }
        let test = Test {
            chain_id: L2ChainId(200),
        };
        let result_ser = serde_json::to_string(&test).unwrap();
        let result_deser: Test = serde_json::from_str(&result_ser).unwrap();
        assert_eq!(test.chain_id, result_deser.chain_id);
        assert_eq!(result_ser, "{\"chain_id\":200}")
    }

    #[test]
    fn test_serialize_deserialize_bincode() {
        #[derive(Serialize, Deserialize)]
        struct Test {
            chain_id: L2ChainId,
        }
        let test = Test {
            chain_id: L2ChainId(200),
        };
        let result_ser = bincode::serialize(&test).unwrap();
        let result_deser: Test = bincode::deserialize(&result_ser).unwrap();
        assert_eq!(test.chain_id, result_deser.chain_id);
    }
    #[test]
    fn test_from_str_valid_hexadecimal() {
        let input = "0x2A";
        let result = L2ChainId::from_str(input);
        assert_eq!(result.unwrap().as_u64(), 42);
    }

    #[test]
    fn test_from_str_too_big_chain_id() {
        let input = "18446744073709551615"; // 2^64 - 1
        let result = L2ChainId::from_str(input);
        assert_eq!(
            result,
            Err(format!(
                "Cannot convert given value {} into L2ChainId. It's greater than MAX: {}",
                input,
                L2ChainId::max().0
            ))
        );
    }

    #[test]
    fn test_from_str_invalid_input() {
        let input = "invalid"; // Invalid input that cannot be parsed as a number
        let result = L2ChainId::from_str(input);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Failed to parse L2ChainId: Err "));
    }

    #[test]
    fn test_deserialize_valid_decimal() {
        let input_json = "\"42\"";

        let result: Result<L2ChainId, _> = from_str(input_json);
        assert_eq!(result.unwrap().as_u64(), 42);
    }

    #[test]
    fn test_deserialize_valid_hex() {
        let input_json = "\"0x2A\"";

        let result: Result<L2ChainId, _> = from_str(input_json);
        assert_eq!(result.unwrap().as_u64(), 42);
    }

    #[test]
    fn test_deserialize_invalid() {
        let input_json = "\"invalid\"";

        let result: Result<L2ChainId, serde_json::Error> = from_str(input_json);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Failed to parse L2ChainId: Err Invalid character "));
    }
}
