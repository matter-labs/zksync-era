use std::str::FromStr;

use zksync_types::{transaction_request::PaymasterParams, Address, U256};

use crate::sdk::ethabi::{Contract, Token};

const IPAYMASTER_FLOW_INTERFACE: &str = include_str!("./abi/IPaymasterFlow.json");

pub fn is_token_eth(token_address: Address) -> bool {
    token_address == Address::zero()
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

/// Returns the approval based paymaster input to be used for estimation of transactions.
/// Note, that the `min_allowance` will be approved to paymaster contract during estimation and so for
/// instance "low" values like zero could lead to underestimation of the transaction (because the cost per
/// write depends on the size of the value).
///
/// This is a chicken-and-egg problem, because we need to know the cost of the transaction to estimate it, so in order to provide
/// the most correct estimate possible it is recommended to set as realistic `min_allowance` as possible.
pub fn get_approval_based_paymaster_input_for_estimation(
    paymaster: Address,
    token_address: Address,
    min_allowance: U256,
) -> PaymasterParams {
    get_approval_based_paymaster_input(paymaster, token_address, min_allowance, Default::default())
}
