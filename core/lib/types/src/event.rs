use crate::{
    ethabi,
    tokens::{TokenInfo, TokenMetadata},
    Address, L1BatchNumber, CONTRACT_DEPLOYER_ADDRESS, H256, KNOWN_CODES_STORAGE_ADDRESS,
    L1_MESSENGER_ADDRESS,
};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use zksync_utils::h256_to_account_address;

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct VmEvent {
    pub location: (L1BatchNumber, u32),
    pub address: Address,
    pub indexed_topics: Vec<H256>,
    pub value: Vec<u8>,
}

impl VmEvent {
    pub fn index_keys(&self) -> impl Iterator<Item = VmEventGroupKey> + '_ {
        self.indexed_topics
            .iter()
            .enumerate()
            .map(move |(idx, &topic)| VmEventGroupKey {
                address: self.address,
                topic: (idx as u32, topic),
            })
    }
}

pub static DEPLOY_EVENT_SIGNATURE: Lazy<H256> = Lazy::new(|| {
    ethabi::long_signature(
        "ContractDeployed",
        &[
            ethabi::ParamType::Address,
            ethabi::ParamType::FixedBytes(32),
            ethabi::ParamType::Address,
        ],
    )
});

static L1_MESSAGE_EVENT_SIGNATURE: Lazy<H256> = Lazy::new(|| {
    ethabi::long_signature(
        "L1MessageSent",
        &[
            ethabi::ParamType::Address,
            ethabi::ParamType::FixedBytes(32),
            ethabi::ParamType::Bytes,
        ],
    )
});

static BRIDGE_INITIALIZATION_SIGNATURE_OLD: Lazy<H256> = Lazy::new(|| {
    ethabi::long_signature(
        "BridgeInitialization",
        &[
            ethabi::ParamType::Address,
            ethabi::ParamType::String,
            ethabi::ParamType::String,
            ethabi::ParamType::Uint(8),
        ],
    )
});

static BRIDGE_INITIALIZATION_SIGNATURE_NEW: Lazy<H256> = Lazy::new(|| {
    ethabi::long_signature(
        "BridgeInitialize",
        &[
            ethabi::ParamType::Address,
            ethabi::ParamType::String,
            ethabi::ParamType::String,
            ethabi::ParamType::Uint(8),
        ],
    )
});

static PUBLISHED_BYTECODE_SIGNATURE: Lazy<H256> = Lazy::new(|| {
    ethabi::long_signature(
        "MarkedAsKnown",
        &[ethabi::ParamType::FixedBytes(32), ethabi::ParamType::Bool],
    )
});

// moved from Runtime Context
pub fn extract_added_tokens(
    l2_erc20_bridge_addr: Address,
    all_generated_events: &[VmEvent],
) -> Vec<TokenInfo> {
    let deployed_tokens = all_generated_events
        .iter()
        .filter(|event| {
            // Filter events from the deployer contract that match the expected signature.
            event.address == CONTRACT_DEPLOYER_ADDRESS
                && event.indexed_topics.len() == 4
                && event.indexed_topics[0] == *DEPLOY_EVENT_SIGNATURE
                && h256_to_account_address(&event.indexed_topics[1]) == l2_erc20_bridge_addr
        })
        .map(|event| h256_to_account_address(&event.indexed_topics[3]));

    extract_added_token_info_from_addresses(all_generated_events, deployed_tokens)
}

// moved from Runtime Context
fn extract_added_token_info_from_addresses(
    all_generated_events: &[VmEvent],
    deployed_tokens: impl Iterator<Item = Address>,
) -> Vec<TokenInfo> {
    deployed_tokens
        .filter_map(|l2_token_address| {
            all_generated_events
                .iter()
                .find(|event| {
                    event.address == l2_token_address
                        && (event.indexed_topics[0] == *BRIDGE_INITIALIZATION_SIGNATURE_NEW
                            || event.indexed_topics[0] == *BRIDGE_INITIALIZATION_SIGNATURE_OLD)
                })
                .map(|event| {
                    let l1_token_address = h256_to_account_address(&event.indexed_topics[1]);
                    let mut dec_ev = ethabi::decode(
                        &[
                            ethabi::ParamType::String,
                            ethabi::ParamType::String,
                            ethabi::ParamType::Uint(8),
                        ],
                        &event.value,
                    )
                    .unwrap();

                    TokenInfo {
                        l1_address: l1_token_address,
                        l2_address: l2_token_address,
                        metadata: TokenMetadata {
                            name: dec_ev.remove(0).into_string().unwrap(),
                            symbol: dec_ev.remove(0).into_string().unwrap(),
                            decimals: dec_ev.remove(0).into_uint().unwrap().as_u32() as u8,
                        },
                    }
                })
        })
        .collect()
}

// moved from RuntimeContext
// Extracts all the "long" L2->L1 messages that were submitted by the
// L1Messenger contract
pub fn extract_long_l2_to_l1_messages(all_generated_events: &[VmEvent]) -> Vec<Vec<u8>> {
    all_generated_events
        .iter()
        .filter(|event| {
            // Filter events from the deployer contract that match the expected signature.
            event.address == L1_MESSENGER_ADDRESS
                && event.indexed_topics.len() == 3
                && event.indexed_topics[0] == *L1_MESSAGE_EVENT_SIGNATURE
        })
        .map(|event| {
            let decoded_tokens = ethabi::decode(&[ethabi::ParamType::Bytes], &event.value)
                .expect("Failed to decode L1MessageSent message");
            // The `Token` does not implement `Copy` trait, so I had to do it like that:
            let bytes_token = decoded_tokens.into_iter().next().unwrap();
            bytes_token.into_bytes().unwrap()
        })
        .collect()
}

// Extract all bytecodes marked as known on the system contracts
pub fn extract_bytecodes_marked_as_known(all_generated_events: &[VmEvent]) -> Vec<H256> {
    all_generated_events
        .iter()
        .filter(|event| {
            // Filter events from the deployer contract that match the expected signature.
            event.address == KNOWN_CODES_STORAGE_ADDRESS
                && event.indexed_topics.len() == 3
                && event.indexed_topics[0] == *PUBLISHED_BYTECODE_SIGNATURE
        })
        .map(|event| event.indexed_topics[1])
        .collect()
}

// Extract bytecodes that were marked as known on the system contracts and should be published onchain
pub fn extract_published_bytecodes(all_generated_events: &[VmEvent]) -> Vec<H256> {
    all_generated_events
        .iter()
        .filter(|event| {
            // Filter events from the deployer contract that match the expected signature.
            event.address == KNOWN_CODES_STORAGE_ADDRESS
                && event.indexed_topics.len() == 3
                && event.indexed_topics[0] == *PUBLISHED_BYTECODE_SIGNATURE
                && event.indexed_topics[2] != H256::zero()
        })
        .map(|event| event.indexed_topics[1])
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct VmEventGroupKey {
    pub address: Address,
    pub topic: (u32, H256),
}
