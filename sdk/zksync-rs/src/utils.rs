use std::str::FromStr;

use num::BigUint;
use zksync_types::{transaction_request::PaymasterParams, Address, U256};

use crate::web3::ethabi::{Contract, Token};

const IPAYMASTER_FLOW_INTERFACE: &str = include_str!("./abi/IPaymasterFlow.json");

pub fn is_token_eth(token_address: Address) -> bool {
    token_address == Address::zero()
}

/// Converts `U256` into the corresponding `BigUint` value.
pub fn u256_to_biguint(value: U256) -> BigUint {
    let mut bytes = [0u8; 32];
    value.to_little_endian(&mut bytes);
    BigUint::from_bytes_le(&bytes)
}

/// Converts `BigUint` value into the corresponding `U256` value.
pub fn biguint_to_u256(value: BigUint) -> U256 {
    let bytes = value.to_bytes_le();
    U256::from_little_endian(&bytes)
}

pub fn load_contract(raw_abi_string: &str) -> Contract {
    let abi_string = serde_json::Value::from_str(raw_abi_string)
        .expect("Malformed contract abi file")
        .get("abi")
        .expect("Malformed contract abi file")
        .to_string();
    Contract::load(abi_string.as_bytes()).unwrap()
}

pub fn get_approval_based_paymaster_input(
    paymaster: Address,
    token_address: Address,
    min_allowance: U256,
    inner_input: Vec<u8>,
) -> PaymasterParams {
    let paymaster_contract = load_contract(IPAYMASTER_FLOW_INTERFACE);
    let paymaster_input = paymaster_contract
        .function("approvalBased")
        .unwrap()
        .encode_input(&[
            Token::Address(token_address),
            Token::Uint(min_allowance),
            Token::Bytes(inner_input),
        ])
        .unwrap();
    PaymasterParams {
        paymaster,
        paymaster_input,
    }
}

pub fn get_approval_based_paymaster_input_for_estimation(
    paymaster: Address,
    token_address: Address,
) -> PaymasterParams {
    get_approval_based_paymaster_input(
        paymaster,
        token_address,
        Default::default(),
        Default::default(),
    )
}

pub fn get_general_paymaster_input(paymaster: Address, inner_input: Vec<u8>) -> PaymasterParams {
    let paymaster_contract = load_contract(IPAYMASTER_FLOW_INTERFACE);
    let paymaster_input = paymaster_contract
        .function("general")
        .unwrap()
        .encode_input(&[Token::Bytes(inner_input)])
        .unwrap();
    PaymasterParams {
        paymaster,
        paymaster_input,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn biguint_u256_conversion_roundrip(u256: U256) {
        let biguint = u256_to_biguint(u256);
        // Make sure that the string representations are the same.
        assert_eq!(biguint.to_string(), u256.to_string());

        let restored = biguint_to_u256(biguint);
        assert_eq!(u256, restored);
    }

    #[test]
    fn test_zero_conversion() {
        biguint_u256_conversion_roundrip(U256::zero())
    }

    #[test]
    fn test_biguint_u256_conversion() {
        // random value that is big enough
        let u256 = U256::from(1_235_999_123_u64).pow(4u64.into());
        biguint_u256_conversion_roundrip(u256)
    }

    #[test]
    fn test_biguint_with_msb_conversion() {
        // make sure the most significant bit was set
        let u256 = U256::from_big_endian(&[0b11010011; 32]);
        biguint_u256_conversion_roundrip(u256)
    }
}
