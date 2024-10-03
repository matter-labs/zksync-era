use zksync_mini_merkle_tree::MiniMerkleTree;
use zksync_types::{
    ethabi::{self, Token},
    web3::keccak256,
    writes::{compress_state_diffs, StateDiffRecord},
};
use zksync_utils::bytecode::hash_bytecode;

use crate::utils::events::L1MessengerL2ToL1Log;

/// Struct based on which the pubdata blob is formed
#[derive(Debug, Clone, Default)]
pub(crate) struct PubdataInput {
    pub(crate) user_logs: Vec<L1MessengerL2ToL1Log>,
    pub(crate) l2_to_l1_messages: Vec<Vec<u8>>,
    pub(crate) published_bytecodes: Vec<Vec<u8>>,
    pub(crate) state_diffs: Vec<StateDiffRecord>,
    pub(crate) l2_to_l1_logs_tree_size: usize,
}

impl PubdataInput {
    pub(crate) fn build_pubdata_pre_gateway(self, with_uncompressed_state_diffs: bool) -> Vec<u8> {
        let mut l1_messenger_pubdata = vec![];

        let PubdataInput {
            user_logs,
            l2_to_l1_messages,
            published_bytecodes,
            state_diffs,
            ..
        } = self;

        // Encoding user L2->L1 logs.
        // Format: `[(numberOfL2ToL1Logs as u32) || l2tol1logs[1] || ... || l2tol1logs[n]]`
        l1_messenger_pubdata.extend((user_logs.len() as u32).to_be_bytes());
        for l2tol1log in user_logs {
            l1_messenger_pubdata.extend(l2tol1log.packed_encoding());
        }

        // Encoding L2->L1 messages
        // Format: `[(numberOfMessages as u32) || (messages[1].len() as u32) || messages[1] || ... || (messages[n].len() as u32) || messages[n]]`
        l1_messenger_pubdata.extend((l2_to_l1_messages.len() as u32).to_be_bytes());
        for message in l2_to_l1_messages {
            l1_messenger_pubdata.extend((message.len() as u32).to_be_bytes());
            l1_messenger_pubdata.extend(message);
        }

        // Encoding bytecodes
        // Format: `[(numberOfBytecodes as u32) || (bytecodes[1].len() as u32) || bytecodes[1] || ... || (bytecodes[n].len() as u32) || bytecodes[n]]`
        l1_messenger_pubdata.extend((published_bytecodes.len() as u32).to_be_bytes());
        for bytecode in published_bytecodes {
            l1_messenger_pubdata.extend((bytecode.len() as u32).to_be_bytes());
            l1_messenger_pubdata.extend(bytecode);
        }

        // Encoding state diffs
        // Format: `[size of compressed state diffs u32 || compressed state diffs || (# state diffs: intial + repeated) as u32 || sorted state diffs by <index, address, key>]`
        let state_diffs_compressed = compress_state_diffs(state_diffs.clone());
        l1_messenger_pubdata.extend(state_diffs_compressed);

        if with_uncompressed_state_diffs {
            l1_messenger_pubdata.extend((state_diffs.len() as u32).to_be_bytes());
            for state_diff in state_diffs {
                l1_messenger_pubdata.extend(state_diff.encode_padded());
            }
        }

        l1_messenger_pubdata
    }
}

pub trait PubdataBuilder {
    // when `l2_version` is true it will return the data to be sent to the L1_MESSENGER
    // otherwise it returns the array of bytes to be sent to settlement layer inside the operator input.
    fn build_pubdata(&self, input: PubdataInput, l2_version: bool) -> Vec<u8>;
}

#[derive(Debug)]
pub struct RollupPubdataBuilder;

impl PubdataBuilder for RollupPubdataBuilder {
    fn build_pubdata(&self, input: PubdataInput, l2_version: bool) -> Vec<u8> {
        let mut l1_messenger_pubdata = vec![];
        let mut l2_da_header = vec![];

        let PubdataInput {
            user_logs,
            l2_to_l1_messages,
            published_bytecodes,
            state_diffs,
            l2_to_l1_logs_tree_size,
        } = input;

        if l2_version {
            let chained_log_hash = build_chained_log_hash(user_logs.clone());
            let log_root_hash = build_logs_root(user_logs.clone(), l2_to_l1_logs_tree_size);
            let chained_msg_hash = build_chained_message_hash(l2_to_l1_messages.clone());
            let chained_bytecodes_hash = build_chained_bytecode_hash(published_bytecodes.clone());

            l2_da_header.push(Token::FixedBytes(chained_log_hash));
            l2_da_header.push(Token::FixedBytes(log_root_hash));
            l2_da_header.push(Token::FixedBytes(chained_msg_hash));
            l2_da_header.push(Token::FixedBytes(chained_bytecodes_hash));
        }

        l1_messenger_pubdata.extend(encode_user_logs(user_logs));

        // Encoding L2->L1 messages
        // Format: `[(numberOfMessages as u32) || (messages[1].len() as u32) || messages[1] || ... || (messages[n].len() as u32) || messages[n]]`
        l1_messenger_pubdata.extend((l2_to_l1_messages.len() as u32).to_be_bytes());
        for message in l2_to_l1_messages {
            l1_messenger_pubdata.extend((message.len() as u32).to_be_bytes());
            l1_messenger_pubdata.extend(message);
        }
        // Encoding bytecodes
        // Format: `[(numberOfBytecodes as u32) || (bytecodes[1].len() as u32) || bytecodes[1] || ... || (bytecodes[n].len() as u32) || bytecodes[n]]`
        l1_messenger_pubdata.extend((published_bytecodes.len() as u32).to_be_bytes());
        for bytecode in published_bytecodes {
            l1_messenger_pubdata.extend((bytecode.len() as u32).to_be_bytes());
            l1_messenger_pubdata.extend(bytecode);
        }
        // Encoding state diffs
        // Format: `[size of compressed state diffs u32 || compressed state diffs || (# state diffs: intial + repeated) as u32 || sorted state diffs by <index, address, key>]`
        let state_diffs_compressed = compress_state_diffs(state_diffs.clone());
        l1_messenger_pubdata.extend(state_diffs_compressed);

        if l2_version {
            l1_messenger_pubdata.extend((state_diffs.len() as u32).to_be_bytes());
            for state_diff in state_diffs {
                l1_messenger_pubdata.extend(state_diff.encode_padded());
            }

            // Selector of `IL2DAValidator::validatePubdata`.
            let func_selector = vec![0x89, 0xf9, 0xa0, 0x72];

            l2_da_header.push(ethabi::Token::Bytes(l1_messenger_pubdata));

            l1_messenger_pubdata = [func_selector, ethabi::encode(&l2_da_header)]
                .concat()
                .to_vec();
        }

        l1_messenger_pubdata
    }
}

#[derive(Debug)]
pub struct ValidiumPubdataBuilder;

impl PubdataBuilder for ValidiumPubdataBuilder {
    fn build_pubdata(&self, input: PubdataInput, l2_version: bool) -> Vec<u8> {
        let mut l1_messenger_pubdata = vec![];
        let mut l2_da_header = vec![];

        let PubdataInput {
            user_logs,
            l2_to_l1_messages,
            published_bytecodes,
            state_diffs,
            l2_to_l1_logs_tree_size,
        } = input;

        if l2_version {
            let chained_log_hash = build_chained_log_hash(user_logs.clone());
            let log_root_hash = build_logs_root(user_logs.clone(), l2_to_l1_logs_tree_size);
            let chained_msg_hash = build_chained_message_hash(l2_to_l1_messages.clone());
            let chained_bytecodes_hash = build_chained_bytecode_hash(published_bytecodes.clone());
            l2_da_header.push(Token::FixedBytes(chained_log_hash));
            l2_da_header.push(Token::FixedBytes(log_root_hash));
            l2_da_header.push(Token::FixedBytes(chained_msg_hash));
            l2_da_header.push(Token::FixedBytes(chained_bytecodes_hash));
        }

        l1_messenger_pubdata.extend(encode_user_logs(user_logs));

        if l2_version {
            // Selector of `IL2DAValidator::validatePubdata`.
            let func_selector = vec![0x89, 0xf9, 0xa0, 0x72];

            l2_da_header.push(ethabi::Token::Bytes(l1_messenger_pubdata));

            [func_selector, ethabi::encode(&l2_da_header)]
                .concat()
                .to_vec()
        } else {
            let state_diffs_packed = state_diffs
                .into_iter()
                .flat_map(|diff| diff.encode_padded())
                .collect::<Vec<_>>();

            keccak256(&state_diffs_packed).to_vec()
        }
    }
}

fn build_chained_log_hash(user_logs: Vec<L1MessengerL2ToL1Log>) -> Vec<u8> {
    let mut chained_log_hash = vec![0u8; 32];

    for log in user_logs {
        let log_bytes = log.packed_encoding();
        let hash = keccak256(&log_bytes);

        chained_log_hash = keccak256(&[chained_log_hash, hash.to_vec()].concat()).to_vec();
    }

    chained_log_hash
}

fn build_logs_root(
    user_logs: Vec<L1MessengerL2ToL1Log>,
    l2_to_l1_logs_tree_size: usize,
) -> Vec<u8> {
    let logs = user_logs.iter().map(|log| {
        let encoded = log.packed_encoding();
        let mut slice = [0u8; 88];
        slice.copy_from_slice(&encoded);
        slice
    });
    MiniMerkleTree::new(logs, Some(l2_to_l1_logs_tree_size))
        .merkle_root()
        .as_bytes()
        .to_vec()
}

fn build_chained_message_hash(l2_to_l1_messages: Vec<Vec<u8>>) -> Vec<u8> {
    let mut chained_msg_hash = vec![0u8; 32];

    for msg in l2_to_l1_messages {
        let hash = keccak256(&msg);

        chained_msg_hash = keccak256(&[chained_msg_hash, hash.to_vec()].concat()).to_vec();
    }

    chained_msg_hash
}

fn build_chained_bytecode_hash(published_bytecodes: Vec<Vec<u8>>) -> Vec<u8> {
    let mut chained_bytecode_hash = vec![0u8; 32];

    for bytecode in published_bytecodes {
        let hash = hash_bytecode(&bytecode).to_fixed_bytes();

        chained_bytecode_hash =
            keccak256(&[chained_bytecode_hash, hash.to_vec()].concat()).to_vec();
    }

    chained_bytecode_hash
}

fn encode_user_logs(user_logs: Vec<L1MessengerL2ToL1Log>) -> Vec<u8> {
    // Encoding user L2->L1 logs.
    // Format: `[(numberOfL2ToL1Logs as u32) || l2tol1logs[1] || ... || l2tol1logs[n]]`
    let mut result = vec![];
    result.extend((user_logs.len() as u32).to_be_bytes());
    for l2tol1log in user_logs {
        result.extend(l2tol1log.packed_encoding());
    }
    result
}

#[cfg(test)]
mod tests {
    use zksync_system_constants::{ACCOUNT_CODE_STORAGE_ADDRESS, BOOTLOADER_ADDRESS};
    use zksync_types::{l2_to_l1_log::l2_to_l1_logs_tree_size, ProtocolVersionId};
    use zksync_utils::u256_to_h256;

    use super::*;

    #[test]
    fn test_basic_pubdata_building() {
        // Just using some constant addresses for tests
        let addr1 = BOOTLOADER_ADDRESS;
        let addr2 = ACCOUNT_CODE_STORAGE_ADDRESS;

        let user_logs = vec![L1MessengerL2ToL1Log {
            l2_shard_id: 0,
            is_service: false,
            tx_number_in_block: 0,
            sender: addr1,
            key: 1.into(),
            value: 128.into(),
        }];

        let l2_to_l1_messages = vec![hex::decode("deadbeef").unwrap()];

        let published_bytecodes = vec![hex::decode("aaaabbbb").unwrap()];

        // For covering more cases, we have two state diffs:
        // One with enumeration index present (and so it is a repeated write) and the one without it.
        let state_diffs = vec![
            StateDiffRecord {
                address: addr2,
                key: 155.into(),
                derived_key: u256_to_h256(125.into()).0,
                enumeration_index: 12,
                initial_value: 11.into(),
                final_value: 12.into(),
            },
            StateDiffRecord {
                address: addr2,
                key: 156.into(),
                derived_key: u256_to_h256(126.into()).0,
                enumeration_index: 0,
                initial_value: 0.into(),
                final_value: 14.into(),
            },
        ];

        let input = PubdataInput {
            user_logs,
            l2_to_l1_messages,
            published_bytecodes,
            state_diffs,
            l2_to_l1_logs_tree_size: l2_to_l1_logs_tree_size(ProtocolVersionId::latest()),
        };

        let pubdata =
            ethabi::encode(&[ethabi::Token::Bytes(input.build_pubdata_pre_gateway(true))])[32..]
                .to_vec();

        assert_eq!(hex::encode(pubdata), "00000000000000000000000000000000000000000000000000000000000002c700000001000000000000000000000000000000000000000000008001000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000800000000100000004deadbeef0000000100000004aaaabbbb0100002a040001000000000000000000000000000000000000000000000000000000000000007e090e0000000c0901000000020000000000000000000000000000000000008002000000000000000000000000000000000000000000000000000000000000009b000000000000000000000000000000000000000000000000000000000000007d000000000000000c000000000000000000000000000000000000000000000000000000000000000b000000000000000000000000000000000000000000000000000000000000000c00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000008002000000000000000000000000000000000000000000000000000000000000009c000000000000000000000000000000000000000000000000000000000000007e00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000e000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000");
    }
}
