use std::str::FromStr;

use zksync_mini_merkle_tree::MiniMerkleTree;
use zksync_types::{bytecode::BytecodeHash, web3::keccak256, writes::compress_state_diffs, H256};

use crate::interface::pubdata::{L1MessengerL2ToL1Log, PubdataInput};

pub(crate) fn build_chained_log_hash(user_logs: &[L1MessengerL2ToL1Log]) -> Vec<u8> {
    let mut chained_log_hash = vec![0u8; 32];

    for log in user_logs {
        let log_bytes = log.packed_encoding();
        let hash = keccak256(&log_bytes);

        chained_log_hash = keccak256(&[chained_log_hash, hash.to_vec()].concat()).to_vec();
    }

    chained_log_hash
}

pub(crate) fn build_logs_root(
    user_logs: &[L1MessengerL2ToL1Log],
    l2_to_l1_logs_tree_size: usize,
) -> Vec<u8> {
    let logs = user_logs.iter().map(|log| {
        let encoded = log.packed_encoding();
        let mut slice = [0u8; 88];
        slice.copy_from_slice(&encoded);
        slice
    });
    // kl todo separate by version?
    MiniMerkleTree::new_with_empty_leaf_hash(
        logs,
        Some(l2_to_l1_logs_tree_size),
        H256::from_str("72abee45b59e344af8a6e520241c4744aff26ed411f4c4b00f8af09adada43ba").unwrap(),
    )
    .merkle_root()
    .as_bytes()
    .to_vec()
}

pub(crate) fn build_chained_message_hash(l2_to_l1_messages: &[Vec<u8>]) -> Vec<u8> {
    let mut chained_msg_hash = vec![0u8; 32];

    for msg in l2_to_l1_messages {
        let hash = keccak256(msg);

        chained_msg_hash = keccak256(&[chained_msg_hash, hash.to_vec()].concat()).to_vec();
    }

    chained_msg_hash
}

pub(crate) fn build_chained_bytecode_hash(published_bytecodes: &[Vec<u8>]) -> Vec<u8> {
    let mut chained_bytecode_hash = vec![0u8; 32];

    for bytecode in published_bytecodes {
        let hash = BytecodeHash::for_bytecode(bytecode)
            .value()
            .to_fixed_bytes();
        chained_bytecode_hash =
            keccak256(&[chained_bytecode_hash, hash.to_vec()].concat()).to_vec();
    }

    chained_bytecode_hash
}

pub(crate) fn encode_user_logs(user_logs: &[L1MessengerL2ToL1Log]) -> Vec<u8> {
    // Encoding user L2->L1 logs.
    // Format: `[(numberOfL2ToL1Logs as u32) || l2tol1logs[1] || ... || l2tol1logs[n]]`
    let mut result = vec![];
    result.extend((user_logs.len() as u32).to_be_bytes());
    for l2tol1log in user_logs {
        result.extend(l2tol1log.packed_encoding());
    }
    result
}

pub(crate) fn extend_from_pubdata_input(buffer: &mut Vec<u8>, pubdata_input: &PubdataInput) {
    let PubdataInput {
        user_logs,
        l2_to_l1_messages,
        published_bytecodes,
        state_diffs,
    } = pubdata_input;

    // Adding user L2->L1 logs.
    buffer.extend(encode_user_logs(user_logs));

    // Encoding L2->L1 messages
    // Format: `[(numberOfMessages as u32) || (messages[1].len() as u32) || messages[1] || ... || (messages[n].len() as u32) || messages[n]]`
    buffer.extend((l2_to_l1_messages.len() as u32).to_be_bytes());
    for message in l2_to_l1_messages {
        buffer.extend((message.len() as u32).to_be_bytes());
        buffer.extend(message);
    }
    // Encoding bytecodes
    // Format: `[(numberOfBytecodes as u32) || (bytecodes[1].len() as u32) || bytecodes[1] || ... || (bytecodes[n].len() as u32) || bytecodes[n]]`
    buffer.extend((published_bytecodes.len() as u32).to_be_bytes());
    for bytecode in published_bytecodes {
        buffer.extend((bytecode.len() as u32).to_be_bytes());
        buffer.extend(bytecode);
    }
    // Encoding state diffs
    // Format: `[size of compressed state diffs u32 || compressed state diffs || (# state diffs: intial + repeated) as u32 || sorted state diffs by <index, address, key>]`
    let state_diffs_compressed = compress_state_diffs(state_diffs.clone());
    buffer.extend(state_diffs_compressed);
}
