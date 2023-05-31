//! The declaration of the most primitive types used in zkSync network.
//!
//! Most of them are just re-exported from the `web3` crate.

#[macro_use]
mod macros;

pub mod network;

use serde::{Deserialize, Serialize};
use std::convert::{Infallible, TryFrom, TryInto};
use std::fmt;
use std::num::ParseIntError;
use std::ops::{Add, Deref, DerefMut, Sub};
use std::str::FromStr;

pub use web3;
pub use web3::ethabi;
pub use web3::types::{
    Address, Bytes, Log, TransactionRequest, H128, H160, H2048, H256, U128, U256, U64,
};

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

basic_type!(
    /// zkSync network block sequential index.
    MiniblockNumber,
    u32
);

basic_type!(
    /// zkSync L1 batch sequential index.
    L1BatchNumber,
    u32
);

basic_type!(
    /// Ethereum network block sequential index.
    L1BlockNumber,
    u32
);

basic_type!(
    /// zkSync account nonce.
    Nonce,
    u32
);

basic_type!(
    /// Unique identifier of the priority operation in the zkSync network.
    PriorityOpId,
    u64
);

basic_type!(
    /// ChainId in the Ethereum network.
    L1ChainId,
    u64
);

basic_type!(
    /// ChainId in the ZkSync network.
    L2ChainId,
    u16
);

#[allow(clippy::derivable_impls)]
impl Default for MiniblockNumber {
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

impl Default for L2ChainId {
    fn default() -> Self {
        Self(270)
    }
}

#[allow(clippy::derivable_impls)]
impl Default for PriorityOpId {
    fn default() -> Self {
        Self(0)
    }
}
