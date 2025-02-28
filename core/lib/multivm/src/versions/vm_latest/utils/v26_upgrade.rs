//! This module contains varios v26-upgrade-related functions to be used by the state keeper's
//! unit tests.
//! The correctness of the functionality present here is hard to enforce inside state keeper 
//! directly, so it is done inside unit tests of the multivm.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    str::FromStr,
};

use ethabi::Token;
use zksync_contracts::{l2_asset_router, l2_legacy_shared_bridge};
use zksync_types::{utils::encode_ntv_asset_id, Address, StorageKey, H256, U256};
use zksync_utils::env::Workspace;

#[derive(Debug, PartialEq, Eq)]
pub struct V26TestData {
    pub l1_chain_id: U256,
    pub l1_shared_bridge_address: Address,
    pub l1_token_address: Address,
    pub l2_token_address: Address,
    pub l2_legacy_shared_bridge_address: Address,
    pub l1_aliased_shared_bridge: Address,
}

pub fn get_test_data() -> V26TestData {
    V26TestData {
        l1_chain_id: 1.into(),
        l1_shared_bridge_address: Address::from_str("abacabac00000000000000000000000000000001")
            .unwrap(),
        l1_token_address: Address::from_str("abacabac00000000000000000000000000000002").unwrap(),
        l2_token_address: Address::from_str("de1b93723f716c741b771276465b7b58aa9c44a6").unwrap(),
        l2_legacy_shared_bridge_address: Address::from_str(
            "d316600b6009f94ab776e97f1fc985bc18b4e535",
        )
        .unwrap(),
        l1_aliased_shared_bridge: Address::from_str("bcbdabac00000000000000000000000000001112")
            .unwrap(),
    }
}

fn get_test_v26_path(name: &str) -> PathBuf {
    let core_path = Workspace::locate().core();
    Path::new(&core_path).join(format!(
        "lib/multivm/src/versions/testonly/v26_utils_outputs/{name}"
    ))
}


pub fn trivial_test_storage_logs() -> HashMap<StorageKey, H256> {
    let x: Vec<_> = serde_json::from_str(
        &std::fs::read_to_string(get_test_v26_path("simple-test.json")).unwrap(),
    )
    .unwrap();
    x.into_iter().collect()
}

pub fn post_bridging_test_storage_logs() -> HashMap<StorageKey, H256> {
    let x: Vec<_> = serde_json::from_str(
        &std::fs::read_to_string(get_test_v26_path("post-bridging.json")).unwrap(),
    )
    .unwrap();
    x.into_iter().collect()
}

pub fn post_registration_test_storage_logs() -> HashMap<StorageKey, H256> {
    let x: Vec<_> = serde_json::from_str(
        &std::fs::read_to_string(get_test_v26_path("post-registration.json")).unwrap(),
    )
    .unwrap();
    x.into_iter().collect()
}

fn empty_erc20_metadata() -> Vec<u8> {
    ethabi::encode(&[
        Token::Bytes(vec![]),
        Token::Bytes(vec![]),
        Token::Bytes(vec![]),
    ])
}

pub fn encode_new_finalize_deposit(l1_chain_id: U256, l1_token_address: Address) -> Vec<u8> {
    let contract = l2_asset_router();
    let functions = contract.functions.get("finalizeDeposit").unwrap();
    let finalize_deposit_3_params = functions.iter().find(|f| f.inputs.len() == 3).unwrap();

    let new_token_data = [
        // New encoding version
        vec![0x01_u8],
        ethabi::encode(&[
            Token::Uint(l1_chain_id),
            Token::Bytes(vec![]),
            Token::Bytes(vec![]),
            Token::Bytes(vec![]),
        ]),
    ]
    .concat();

    // The original Solidity code can be found in `DataEncoding`
    let bridge_mint_data = ethabi::encode(&[
        Token::Address(Address::from_low_u64_be(1)),
        Token::Address(Address::from_low_u64_be(2)),
        Token::Address(l1_token_address),
        Token::Uint(U256::from(1u32)),
        Token::Bytes(new_token_data),
    ]);

    let asset_id = encode_ntv_asset_id(l1_chain_id, l1_token_address);

    finalize_deposit_3_params
        .encode_input(&[
            Token::Uint(0.into()),
            Token::FixedBytes(asset_id.0.to_vec()),
            Token::Bytes(bridge_mint_data),
        ])
        .unwrap()
}

pub fn encode_legacy_finalize_deposit(l1_token_address: Address) -> Vec<u8> {
    let legacy_shared_bridge = l2_legacy_shared_bridge();
    legacy_shared_bridge
        .function("finalizeDeposit")
        .unwrap()
        .encode_input(&[
            Token::Address(Address::from_low_u64_be(1)),
            Token::Address(Address::from_low_u64_be(2)),
            Token::Address(l1_token_address),
            Token::Uint(U256::from(1)),
            Token::Bytes(empty_erc20_metadata()),
        ])
        .unwrap()
}
