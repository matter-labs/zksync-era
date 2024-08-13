use zksync_system_constants::L1_MESSENGER_ADDRESS;
use zksync_types::{
    ethabi::{self, Token},
    event::L1_MESSENGER_BYTECODE_PUBLICATION_EVENT_SIGNATURE,
    l2_to_l1_log::L2ToL1Log,
    Address, VmEvent, H256, U256,
};
use zksync_utils::{u256_to_bytes_be, u256_to_h256};

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
pub(crate) struct L1MessengerL2ToL1Log {
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

#[derive(Debug, PartialEq)]
pub(crate) struct L1MessengerBytecodePublicationRequest {
    pub bytecode_hash: H256,
}

/// Extracts all the `L2ToL1Logs` that were emitted by the `L1Messenger` contract.
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

/// Extracts all the bytecode publication requests that were emitted by the L1Messenger contract.
pub(crate) fn extract_bytecode_publication_requests_from_l1_messenger(
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

#[cfg(test)]
mod tests {
    use zksync_system_constants::{
        BOOTLOADER_ADDRESS, KNOWN_CODES_STORAGE_ADDRESS, L2_BASE_TOKEN_ADDRESS,
    };
    use zksync_types::L1BatchNumber;

    use super::*;

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
                L2_BASE_TOKEN_ADDRESS,
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

    fn create_byte_code_publication_req_value(hash: U256) -> Vec<u8> {
        let mut hash_arr = [0u8; 32];
        hash.to_big_endian(&mut hash_arr);

        let tokens = vec![/*bytecode hash*/ Token::FixedBytes(hash_arr.to_vec())];

        ethabi::encode(&tokens)
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
            create_bytecode_publication_vm_event(L2_BASE_TOKEN_ADDRESS, U256::from(1337)),
            create_bytecode_publication_vm_event(L1_MESSENGER_ADDRESS, U256::from(1438284388)),
            create_bytecode_publication_vm_event(L1_MESSENGER_ADDRESS, U256::from(1231014388)),
        ];

        let logs = extract_bytecode_publication_requests_from_l1_messenger(&events);

        assert_eq!(expected, logs);
    }
}
