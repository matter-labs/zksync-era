use zksync_mini_merkle_tree::MiniMerkleTree;
use zksync_types::web3::keccak256;
use zksync_utils::bytecode::hash_bytecode;

use crate::interface::pubdata::L1MessengerL2ToL1Log;

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
    MiniMerkleTree::new(logs, Some(l2_to_l1_logs_tree_size))
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
        let hash = hash_bytecode(bytecode).to_fixed_bytes();

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
