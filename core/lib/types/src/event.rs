use crate::{
    ethabi,
    l2_to_l1_log::L2ToL1Log,
    tokens::{TokenInfo, TokenMetadata},
    Address, L1BatchNumber, CONTRACT_DEPLOYER_ADDRESS, H256, KNOWN_CODES_STORAGE_ADDRESS,
    L1_MESSENGER_ADDRESS, U256,
};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use zksync_basic_types::ethabi::Token;
use zksync_utils::{h256_to_account_address, u256_to_bytes_be, u256_to_h256};

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

static L1_MESSENGER_BYTECODE_PUBLICATION_EVENT_SIGNATURE: Lazy<H256> = Lazy::new(|| {
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

// moved from RuntimeContext
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

// Extracts all the L2ToL1Logs that were emitted
// by the L1Messenger contract
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

#[cfg(test)]
mod tests {
    use zksync_basic_types::{
        ethabi::{self, Token},
        Address, L1BatchNumber, U256,
    };
    use zksync_system_constants::{
        BOOTLOADER_ADDRESS, KNOWN_CODES_STORAGE_ADDRESS, L1_MESSENGER_ADDRESS, L2_ETH_TOKEN_ADDRESS,
    };
    use zksync_utils::u256_to_h256;

    use crate::VmEvent;

    use super::{
        extract_bytecode_publication_requests_from_l1_messenger,
        extract_l2tol1logs_from_l1_messenger, L1MessengerBytecodePublicationRequest,
        L1MessengerL2ToL1Log,
    };

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
            /*l2ShardId*/ Token::Uint(U256::from(0)),
            /*isService*/ Token::Bool(true),
            /*txNumberInBlock*/ Token::Uint(tx_number),
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
}
