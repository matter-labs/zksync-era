use std::fmt::Debug;

use itertools::Itertools;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use zksync_basic_types::ethabi::Token;
use zksync_system_constants::EVENT_WRITER_ADDRESS;
use zksync_utils::{
    address_to_u256, h256_to_account_address, h256_to_u256, u256_to_bytes_be, u256_to_h256,
};

use crate::{
    api::Log,
    ethabi,
    l2_to_l1_log::L2ToL1Log,
    tokens::{TokenInfo, TokenMetadata},
    web3::{Bytes, Index},
    zk_evm_types::{LogQuery, Timestamp},
    Address, L1BatchNumber, CONTRACT_DEPLOYER_ADDRESS, H256, KNOWN_CODES_STORAGE_ADDRESS,
    L1_MESSENGER_ADDRESS, U256, U64,
};

#[cfg(test)]
mod tests;

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

impl From<&VmEvent> for Log {
    fn from(vm_event: &VmEvent) -> Self {
        Log {
            address: vm_event.address,
            topics: vm_event.indexed_topics.clone(),
            data: Bytes::from(vm_event.value.clone()),
            block_hash: None,
            block_number: None,
            l1_batch_number: Some(U64::from(vm_event.location.0 .0)),
            transaction_hash: None,
            transaction_index: Some(Index::from(vm_event.location.1)),
            log_index: None,
            transaction_log_index: None,
            log_type: None,
            removed: Some(false),
        }
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

/// Corresponds to the following solidity event:
/// ```solidity
/// struct L2ToL1Log {
///     uint8 l2ShardId;
///     bool isService;
///     uint16 txNumberInBlock;
///     address sender;
///     bytes32 key;
///     bytes32 value;
/// }
/// ```
#[derive(Debug, Default, Clone, PartialEq)]
pub struct L1MessengerL2ToL1Log {
    pub l2_shard_id: u8,
    pub is_service: bool,
    pub tx_number_in_block: u16,
    pub sender: Address,
    pub key: U256,
    pub value: U256,
}

impl L1MessengerL2ToL1Log {
    pub fn packed_encoding(&self) -> Vec<u8> {
        let mut res: Vec<u8> = vec![];
        res.push(self.l2_shard_id);
        res.push(self.is_service as u8);
        res.extend_from_slice(&self.tx_number_in_block.to_be_bytes());
        res.extend_from_slice(self.sender.as_bytes());
        res.extend(u256_to_bytes_be(&self.key));
        res.extend(u256_to_bytes_be(&self.value));
        res
    }
}

impl From<L1MessengerL2ToL1Log> for L2ToL1Log {
    fn from(log: L1MessengerL2ToL1Log) -> Self {
        L2ToL1Log {
            shard_id: log.l2_shard_id,
            is_service: log.is_service,
            tx_number_in_block: log.tx_number_in_block,
            sender: log.sender,
            key: u256_to_h256(log.key),
            value: u256_to_h256(log.value),
        }
    }
}

pub static L1_MESSENGER_BYTECODE_PUBLICATION_EVENT_SIGNATURE: Lazy<H256> = Lazy::new(|| {
    ethabi::long_signature(
        "BytecodeL1PublicationRequested",
        &[ethabi::ParamType::FixedBytes(32)],
    )
});

pub static MESSAGE_ROOT_ADDED_CHAIN_EVENT: Lazy<H256> = Lazy::new(|| {
    ethabi::long_signature(
        "AddedChain",
        &[ethabi::ParamType::Uint(256), ethabi::ParamType::Uint(256)],
    )
});

pub static MESSAGE_ROOT_ADDED_CHAIN_BATCH_ROOT_EVENT: Lazy<H256> = Lazy::new(|| {
    ethabi::long_signature(
        "AppendedChainBatchRoot",
        &[
            ethabi::ParamType::Uint(256),
            ethabi::ParamType::Uint(256),
            ethabi::ParamType::FixedBytes(32),
        ],
    )
});

#[derive(Debug, PartialEq)]
pub struct L1MessengerBytecodePublicationRequest {
    pub bytecode_hash: H256,
}

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
    l2_shared_bridge_addr: Address,
    all_generated_events: &[VmEvent],
) -> Vec<TokenInfo> {
    let deployed_tokens = all_generated_events
        .iter()
        .filter(|event| {
            // Filter events from the deployer contract that match the expected signature.
            event.address == CONTRACT_DEPLOYER_ADDRESS
                && event.indexed_topics.len() == 4
                && event.indexed_topics[0] == *DEPLOY_EVENT_SIGNATURE
                && h256_to_account_address(&event.indexed_topics[1]) == l2_shared_bridge_addr
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

// moved from `RuntimeContext`
// Extracts all the "long" L2->L1 messages that were submitted by the
// L1Messenger contract
pub fn extract_long_l2_to_l1_messages(all_generated_events: &[VmEvent]) -> Vec<Vec<u8>> {
    all_generated_events
        .iter()
        .filter(|event| {
            // Filter events from the l1 messenger contract that match the expected signature.
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

// Extracts all the `L2ToL1Logs` that were emitted
// by the `L1Messenger` contract
pub fn extract_l2tol1logs_from_l1_messenger(
    all_generated_events: &[VmEvent],
) -> Vec<L1MessengerL2ToL1Log> {
    let params = &[ethabi::ParamType::Tuple(vec![
        ethabi::ParamType::Uint(8),
        ethabi::ParamType::Bool,
        ethabi::ParamType::Uint(16),
        ethabi::ParamType::Address,
        ethabi::ParamType::FixedBytes(32),
        ethabi::ParamType::FixedBytes(32),
    ])];

    let l1_messenger_l2_to_l1_log_event_signature = ethabi::long_signature("L2ToL1LogSent", params);

    all_generated_events
        .iter()
        .filter(|event| {
            // Filter events from the l1 messenger contract that match the expected signature.
            event.address == L1_MESSENGER_ADDRESS
                && !event.indexed_topics.is_empty()
                && event.indexed_topics[0] == l1_messenger_l2_to_l1_log_event_signature
        })
        .map(|event| {
            let tuple = ethabi::decode(params, &event.value)
                .expect("Failed to decode L2ToL1LogSent message")
                .first()
                .unwrap()
                .clone();
            let Token::Tuple(tokens) = tuple else {
                panic!("Tuple was expected, got: {}", tuple);
            };
            let [
                Token::Uint(shard_id),
                Token::Bool(is_service),
                Token::Uint(tx_number_in_block),
                Token::Address(sender),
                Token::FixedBytes(key_bytes),
                Token::FixedBytes(value_bytes),
            ] = tokens.as_slice() else {
                panic!("Invalid tuple types");
            };
            L1MessengerL2ToL1Log {
                l2_shard_id: shard_id.low_u64() as u8,
                is_service: *is_service,
                tx_number_in_block: tx_number_in_block.low_u64() as u16,
                sender: *sender,
                key: U256::from_big_endian(key_bytes),
                value: U256::from_big_endian(value_bytes),
            }
        })
        .collect()
}

// Extracts all the bytecode publication requests
// that were emitted by the L1Messenger contract
pub fn extract_bytecode_publication_requests_from_l1_messenger(
    all_generated_events: &[VmEvent],
) -> Vec<L1MessengerBytecodePublicationRequest> {
    all_generated_events
        .iter()
        .filter(|event| {
            // Filter events from the l1 messenger contract that match the expected signature.
            event.address == L1_MESSENGER_ADDRESS
                && !event.indexed_topics.is_empty()
                && event.indexed_topics[0] == *L1_MESSENGER_BYTECODE_PUBLICATION_EVENT_SIGNATURE
        })
        .map(|event| {
            let mut tokens = ethabi::decode(&[ethabi::ParamType::FixedBytes(32)], &event.value)
                .expect("Failed to decode BytecodeL1PublicationRequested message");
            L1MessengerBytecodePublicationRequest {
                bytecode_hash: H256::from_slice(&tokens.remove(0).into_fixed_bytes().unwrap()),
            }
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

/// Each `VmEvent` can be translated to several log queries.
/// This methods converts each event from input to log queries and returns all produced log queries.
pub fn convert_vm_events_to_log_queries(events: &[VmEvent]) -> Vec<LogQuery> {
    events
        .iter()
        .flat_map(|event| {
            // Construct first query. This query holds an information about
            // - number of event topics (on log query level `event.address` is treated as a topic, thus + 1 is added)
            // - length of event value
            // - `event.address` (or first topic in terms of log query terminology).
            let first_key_word =
                (event.indexed_topics.len() as u64 + 1) + ((event.value.len() as u64) << 32);
            let key = U256([first_key_word, 0, 0, 0]);

            // `timestamp`, `aux_byte`, `read_value`, `rw_flag`, `rollback` are set as per convention.
            let first_log = LogQuery {
                timestamp: Timestamp(0),
                tx_number_in_block: event.location.1 as u16,
                aux_byte: 0,
                shard_id: 0,
                address: EVENT_WRITER_ADDRESS,
                key,
                read_value: U256::zero(),
                written_value: address_to_u256(&event.address),
                rw_flag: false,
                rollback: false,
                is_service: true,
            };

            // The next logs hold information about remaining topics and `event.value`.
            // Each log can hold at most two values each of 32 bytes.
            // The following piece of code prepares these 32-byte values.
            let values = event.indexed_topics.iter().map(|h| h256_to_u256(*h)).chain(
                event.value.chunks(32).map(|value_chunk| {
                    let mut padded = value_chunk.to_vec();
                    padded.resize(32, 0);
                    U256::from_big_endian(&padded)
                }),
            );

            // And now we process these values in chunks by two.
            let value_chunks = values.chunks(2);
            let other_logs = value_chunks.into_iter().map(|mut chunk| {
                // The first value goes to `log_query.key`.
                let key = chunk.next().unwrap();

                // If the second one is present then it goes to `log_query.written_value`.
                let written_value = chunk.next().unwrap_or_default();

                LogQuery {
                    timestamp: Timestamp(0),
                    tx_number_in_block: event.location.1 as u16,
                    aux_byte: 0,
                    shard_id: 0,
                    address: EVENT_WRITER_ADDRESS,
                    key,
                    read_value: U256::zero(),
                    written_value,
                    rw_flag: false,
                    rollback: false,
                    is_service: false,
                }
            });

            std::iter::once(first_log)
                .chain(other_logs)
                .collect::<Vec<_>>()
        })
        .collect()
}
