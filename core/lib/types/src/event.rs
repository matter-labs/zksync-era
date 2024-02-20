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
    ethabi,
    l2_to_l1_log::L2ToL1Log,
    tokens::{TokenInfo, TokenMetadata},
    zk_evm_types::{LogQuery, Timestamp},
    Address, L1BatchNumber, CONTRACT_DEPLOYER_ADDRESS, H256, KNOWN_CODES_STORAGE_ADDRESS,
    L1_MESSENGER_ADDRESS, U256,
};

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

            // The next logs hold an information about remaining topics and `event.value`.
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

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use zksync_system_constants::{BOOTLOADER_ADDRESS, L2_ETH_TOKEN_ADDRESS};

    use super::*;

    fn create_l2_to_l1_log_sent_value(
        tx_number: U256,
        sender: Address,
        key: U256,
        value: U256,
    ) -> Vec<u8> {
        let mut key_arr = [0u8; 32];
        key.to_big_endian(&mut key_arr);

        let mut val_arr = [0u8; 32];
        value.to_big_endian(&mut val_arr);

        let tokens = vec![
            /*`l2ShardId`*/ Token::Uint(U256::from(0)),
            /*`isService`*/ Token::Bool(true),
            /*`txNumberInBlock`*/ Token::Uint(tx_number),
            /*sender*/ Token::Address(sender),
            /*key*/ Token::FixedBytes(key_arr.to_vec()),
            /*value*/ Token::FixedBytes(val_arr.to_vec()),
        ];

        ethabi::encode(&tokens)
    }

    fn create_byte_code_publication_req_value(hash: U256) -> Vec<u8> {
        let mut hash_arr = [0u8; 32];
        hash.to_big_endian(&mut hash_arr);

        let tokens = vec![/*bytecode hash*/ Token::FixedBytes(hash_arr.to_vec())];

        ethabi::encode(&tokens)
    }

    fn create_l2_to_l1_log_vm_event(
        from: Address,
        tx_number: U256,
        sender: Address,
        key: U256,
        value: U256,
    ) -> VmEvent {
        let l1_messenger_l2_to_l1_log_event_signature = ethabi::long_signature(
            "L2ToL1LogSent",
            &[ethabi::ParamType::Tuple(vec![
                ethabi::ParamType::Uint(8),
                ethabi::ParamType::Bool,
                ethabi::ParamType::Uint(16),
                ethabi::ParamType::Address,
                ethabi::ParamType::FixedBytes(32),
                ethabi::ParamType::FixedBytes(32),
            ])],
        );

        VmEvent {
            location: (L1BatchNumber(1), 0u32),
            address: from,
            indexed_topics: vec![l1_messenger_l2_to_l1_log_event_signature],
            value: create_l2_to_l1_log_sent_value(tx_number, sender, key, value),
        }
    }

    fn create_bytecode_publication_vm_event(from: Address, hash: U256) -> VmEvent {
        let bytecode_publication_event_signature = ethabi::long_signature(
            "BytecodeL1PublicationRequested",
            &[ethabi::ParamType::FixedBytes(32)],
        );

        VmEvent {
            location: (L1BatchNumber(1), 0u32),
            address: from,
            indexed_topics: vec![bytecode_publication_event_signature],
            value: create_byte_code_publication_req_value(hash),
        }
    }

    #[test]
    fn test_extract_l2tol1logs_from_l1_messenger() {
        let expected = vec![
            L1MessengerL2ToL1Log {
                l2_shard_id: 0u8,
                is_service: true,
                tx_number_in_block: 5u16,
                sender: KNOWN_CODES_STORAGE_ADDRESS,
                key: U256::from(11),
                value: U256::from(19),
            },
            L1MessengerL2ToL1Log {
                l2_shard_id: 0u8,
                is_service: true,
                tx_number_in_block: 7u16,
                sender: L1_MESSENGER_ADDRESS,
                key: U256::from(19),
                value: U256::from(93),
            },
        ];

        let events = vec![
            create_l2_to_l1_log_vm_event(
                L1_MESSENGER_ADDRESS,
                U256::from(5),
                KNOWN_CODES_STORAGE_ADDRESS,
                U256::from(11),
                U256::from(19),
            ),
            create_l2_to_l1_log_vm_event(
                BOOTLOADER_ADDRESS,
                U256::from(6),
                L2_ETH_TOKEN_ADDRESS,
                U256::from(6),
                U256::from(8),
            ),
            create_l2_to_l1_log_vm_event(
                L1_MESSENGER_ADDRESS,
                U256::from(7),
                L1_MESSENGER_ADDRESS,
                U256::from(19),
                U256::from(93),
            ),
        ];

        let logs = extract_l2tol1logs_from_l1_messenger(&events);

        assert_eq!(expected, logs);
    }

    #[test]
    fn test_extract_bytecode_publication_requests_from_l1_messenger() {
        let expected = vec![
            L1MessengerBytecodePublicationRequest {
                bytecode_hash: u256_to_h256(U256::from(1438284388)),
            },
            L1MessengerBytecodePublicationRequest {
                bytecode_hash: u256_to_h256(U256::from(1231014388)),
            },
        ];

        let events = vec![
            create_bytecode_publication_vm_event(L2_ETH_TOKEN_ADDRESS, U256::from(1337)),
            create_bytecode_publication_vm_event(L1_MESSENGER_ADDRESS, U256::from(1438284388)),
            create_bytecode_publication_vm_event(L1_MESSENGER_ADDRESS, U256::from(1231014388)),
        ];

        let logs = extract_bytecode_publication_requests_from_l1_messenger(&events);

        assert_eq!(expected, logs);
    }

    #[test]
    fn test_convert_vm_events_to_log_queries() {
        let event_with_1_topic_and_long_value = VmEvent {
            location: (Default::default(), 0),
            address: Address::from_str("0x0000000000000000000000000000000000008008").unwrap(),
            indexed_topics: vec![H256::from_str("0x27FE8C0B49F49507B9D4FE5968C9F49EDFE5C9DF277D433A07A0717EDE97638D").unwrap()],
            value: hex::decode("0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000008001835D4D504D1BF838056AECA450A2768B400A55FF247B31B29E98CC392BAE71190000000000000000000000000000000000000000000000000000000000000001").unwrap(),
        };

        let expected_list: Vec<LogQuery> = serde_json::from_str(r#"[{"key": "0xc000000002", "address": "0x000000000000000000000000000000000000800d", "rw_flag": false, "aux_byte": 0, "rollback": false, "shard_id": 0, "timestamp": 0, "is_service": true, "read_value": "0x0", "written_value": "0x8008", "tx_number_in_block": 0}, {"key": "0x27fe8c0b49f49507b9d4fe5968c9f49edfe5c9df277d433a07a0717ede97638d", "address": "0x000000000000000000000000000000000000800d", "rw_flag": false, "aux_byte": 0, "rollback": false, "shard_id": 0, "timestamp": 0, "is_service": false, "read_value": "0x0", "written_value": "0x0", "tx_number_in_block": 0}, {"key": "0x1", "address": "0x000000000000000000000000000000000000800d", "rw_flag": false, "aux_byte": 0, "rollback": false, "shard_id": 0, "timestamp": 0, "is_service": false, "read_value": "0x0", "written_value": "0x0", "tx_number_in_block": 0}, {"key": "0x8001", "address": "0x000000000000000000000000000000000000800d", "rw_flag": false, "aux_byte": 0, "rollback": false, "shard_id": 0, "timestamp": 0, "is_service": false, "read_value": "0x0", "written_value": "0x835d4d504d1bf838056aeca450a2768b400a55ff247b31b29e98cc392bae7119", "tx_number_in_block": 0}, {"key": "0x1", "address": "0x000000000000000000000000000000000000800d", "rw_flag": false, "aux_byte": 0, "rollback": false, "shard_id": 0, "timestamp": 0, "is_service": false, "read_value": "0x0", "written_value": "0x0", "tx_number_in_block": 0}]"#).unwrap();
        let actual_list = convert_vm_events_to_log_queries(&[event_with_1_topic_and_long_value]);
        assert_eq!(actual_list, expected_list);

        let event_with_2_topics = VmEvent {
            location: (Default::default(), 0),
            address: Address::from_str("0x000000000000000000000000000000000000800A").unwrap(),
            indexed_topics: vec![
                H256::from_str(
                    "0x0F6798A560793A54C3BCFE86A93CDE1E73087D944C0EA20544137D4121396885",
                )
                .unwrap(),
                H256::from_str(
                    "0x00000000000000000000000036615CF349D7F6344891B1E7CA7C72883F5DC049",
                )
                .unwrap(),
            ],
            value: hex::decode("00000000000000000000000000000000000000000000000068155A43676E0000")
                .unwrap(),
        };

        let expected_list: Vec<LogQuery> = serde_json::from_str(r#"[{"key": "0x2000000003", "address": "0x000000000000000000000000000000000000800d", "rw_flag": false, "aux_byte": 0, "rollback": false, "shard_id": 0, "timestamp": 0, "is_service": true, "read_value": "0x0", "written_value": "0x800a", "tx_number_in_block": 0}, {"key": "0xf6798a560793a54c3bcfe86a93cde1e73087d944c0ea20544137d4121396885", "address": "0x000000000000000000000000000000000000800d", "rw_flag": false, "aux_byte": 0, "rollback": false, "shard_id": 0, "timestamp": 0, "is_service": false, "read_value": "0x0", "written_value": "0x36615cf349d7f6344891b1e7ca7c72883f5dc049", "tx_number_in_block": 0}, {"key": "0x68155a43676e0000", "address": "0x000000000000000000000000000000000000800d", "rw_flag": false, "aux_byte": 0, "rollback": false, "shard_id": 0, "timestamp": 0, "is_service": false, "read_value": "0x0", "written_value": "0x0", "tx_number_in_block": 0}]"#).unwrap();
        let actual_list = convert_vm_events_to_log_queries(&[event_with_2_topics]);
        assert_eq!(actual_list, expected_list);

        let event_with_3_topics = VmEvent {
            location: (Default::default(), 0),
            address: Address::from_str("0x0000000000000000000000000000000000008004").unwrap(),
            indexed_topics: vec![
                H256::from_str(
                    "0xC94722FF13EACF53547C4741DAB5228353A05938FFCDD5D4A2D533AE0E618287",
                )
                .unwrap(),
                H256::from_str(
                    "0x010000691FA4F751F8312BC555242F18ED78CDC9AABC0EA77D7D5A675EE8AC6F",
                )
                .unwrap(),
                H256::from_str(
                    "0x0000000000000000000000000000000000000000000000000000000000000000",
                )
                .unwrap(),
            ],
            value: Vec::new(),
        };

        let expected_list: Vec<LogQuery> = serde_json::from_str(r#"[{"key": "0x4", "address": "0x000000000000000000000000000000000000800d", "rw_flag": false, "aux_byte": 0, "rollback": false, "shard_id": 0, "timestamp": 0, "is_service": true, "read_value": "0x0", "written_value": "0x8004", "tx_number_in_block": 0}, {"key": "0xc94722ff13eacf53547c4741dab5228353a05938ffcdd5d4a2d533ae0e618287", "address": "0x000000000000000000000000000000000000800d", "rw_flag": false, "aux_byte": 0, "rollback": false, "shard_id": 0, "timestamp": 0, "is_service": false, "read_value": "0x0", "written_value": "0x10000691fa4f751f8312bc555242f18ed78cdc9aabc0ea77d7d5a675ee8ac6f", "tx_number_in_block": 0}, {"key": "0x0", "address": "0x000000000000000000000000000000000000800d", "rw_flag": false, "aux_byte": 0, "rollback": false, "shard_id": 0, "timestamp": 0, "is_service": false, "read_value": "0x0", "written_value": "0x0", "tx_number_in_block": 0}]"#).unwrap();
        let actual_list = convert_vm_events_to_log_queries(&[event_with_3_topics]);
        assert_eq!(actual_list, expected_list);

        let event_with_4_topics = VmEvent {
            location: (Default::default(), 1),
            address: Address::from_str("0x0000000000000000000000000000000000008006").unwrap(),
            indexed_topics: vec![
                H256::from_str(
                    "0x290AFDAE231A3FC0BBAE8B1AF63698B0A1D79B21AD17DF0342DFB952FE74F8E5",
                )
                .unwrap(),
                H256::from_str(
                    "0x0000000000000000000000004AB56CD027A9AC1E5408D1290E24FD4624B4E8D1",
                )
                .unwrap(),
                H256::from_str(
                    "0x010001E3C92E1345FB2A4EC50F5B99C01DEAB45EEB81EAB41D0F0AB8FF2F58B4",
                )
                .unwrap(),
                H256::from_str(
                    "0x000000000000000000000000E3C0F6A36D71E4F7354A8FB319474BC7E5D822CA",
                )
                .unwrap(),
            ],
            value: Vec::new(),
        };

        let expected_list: Vec<LogQuery> = serde_json::from_str(r#"[{"key": "0x5", "address": "0x000000000000000000000000000000000000800d", "rw_flag": false, "aux_byte": 0, "rollback": false, "shard_id": 0, "timestamp": 0, "is_service": true, "read_value": "0x0", "written_value": "0x8006", "tx_number_in_block": 1}, {"key": "0x290afdae231a3fc0bbae8b1af63698b0a1d79b21ad17df0342dfb952fe74f8e5", "address": "0x000000000000000000000000000000000000800d", "rw_flag": false, "aux_byte": 0, "rollback": false, "shard_id": 0, "timestamp": 0, "is_service": false, "read_value": "0x0", "written_value": "0x4ab56cd027a9ac1e5408d1290e24fd4624b4e8d1", "tx_number_in_block": 1}, {"key": "0x10001e3c92e1345fb2a4ec50f5b99c01deab45eeb81eab41d0f0ab8ff2f58b4", "address": "0x000000000000000000000000000000000000800d", "rw_flag": false, "aux_byte": 0, "rollback": false, "shard_id": 0, "timestamp": 0, "is_service": false, "read_value": "0x0", "written_value": "0xe3c0f6a36d71e4f7354a8fb319474bc7e5d822ca", "tx_number_in_block": 1}]"#).unwrap();
        let actual_list = convert_vm_events_to_log_queries(&[event_with_4_topics]);
        assert_eq!(actual_list, expected_list);

        let event_with_value_len_1 = VmEvent {
            location: (Default::default(), 1),
            address: Address::from_str("0xED38C34B90B125D6B59C02B8AC7ED8387876A8C8").unwrap(),
            indexed_topics: vec![H256::from_str(
                "0x1111111111111111111111111111111111111111111111111111111111111111",
            )
            .unwrap()],
            value: hex::decode("11").unwrap(),
        };

        let expected_list: Vec<LogQuery> = serde_json::from_str(r#"[{"key": "0x100000002", "address": "0x000000000000000000000000000000000000800d", "rw_flag": false, "aux_byte": 0, "rollback": false, "shard_id": 0, "timestamp": 0, "is_service": true, "read_value": "0x0", "written_value": "0xed38c34b90b125d6b59c02b8ac7ed8387876a8c8", "tx_number_in_block": 1}, {"key": "0x1111111111111111111111111111111111111111111111111111111111111111", "address": "0x000000000000000000000000000000000000800d", "rw_flag": false, "aux_byte": 0, "rollback": false, "shard_id": 0, "timestamp": 0, "is_service": false, "read_value": "0x0", "written_value": "0x1100000000000000000000000000000000000000000000000000000000000000", "tx_number_in_block": 1}]"#).unwrap();
        let actual_list = convert_vm_events_to_log_queries(&[event_with_value_len_1]);
        assert_eq!(actual_list, expected_list);
    }
}
