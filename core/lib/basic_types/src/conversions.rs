//! Conversions between basic types.

use crate::{Address, H256, U256};

pub fn h256_to_u256(num: H256) -> U256 {
    U256::from_big_endian(num.as_bytes())
}

pub fn address_to_h256(address: &Address) -> H256 {
    let mut buffer = [0u8; 32];
    buffer[12..].copy_from_slice(address.as_bytes());
    H256(buffer)
}

pub fn address_to_u256(address: &Address) -> U256 {
    h256_to_u256(address_to_h256(address))
}

pub fn u256_to_h256(num: U256) -> H256 {
    let mut bytes = [0u8; 32];
    num.to_big_endian(&mut bytes);
    H256::from_slice(&bytes)
}

/// Converts `U256` value into an [`Address`].
pub fn u256_to_address(value: &U256) -> Address {
    let mut bytes = [0u8; 32];
    value.to_big_endian(&mut bytes);

    Address::from_slice(&bytes[12..])
}

/// Converts `H256` value into an [`Address`].
pub fn h256_to_address(value: &H256) -> Address {
    Address::from_slice(&value.as_bytes()[12..])
}
