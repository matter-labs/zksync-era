use zksync_system_constants::{
    SYSTEM_CONTEXT_ADDRESS, SYSTEM_CONTEXT_CURRENT_L2_BLOCK_HASHES_POSITION,
    SYSTEM_CONTEXT_CURRENT_L2_BLOCK_INFO_POSITION, SYSTEM_CONTEXT_CURRENT_TX_ROLLING_HASH_POSITION,
    SYSTEM_CONTEXT_STORED_L2_BLOCK_HASHES,
};
use zksync_types::{
    block::unpack_block_info, h256_to_u256, u256_to_h256, web3::keccak256, AccountTreeId,
    L2BlockNumber, ProtocolVersionId, StorageKey, H256, U256,
};

use crate::interface::{
    storage::{ReadStorage, StoragePtr},
    L2Block, L2BlockEnv,
};

pub(crate) fn get_l2_block_hash_key(block_number: u32) -> StorageKey {
    let position = h256_to_u256(SYSTEM_CONTEXT_CURRENT_L2_BLOCK_HASHES_POSITION)
        + U256::from(block_number % SYSTEM_CONTEXT_STORED_L2_BLOCK_HASHES);
    StorageKey::new(
        AccountTreeId::new(SYSTEM_CONTEXT_ADDRESS),
        u256_to_h256(position),
    )
}

pub(crate) fn assert_next_block(
    prev_block: &L2Block,
    next_block: &L2BlockEnv,
    protocol_version: ProtocolVersionId,
) {
    if prev_block.number == 0 {
        // Special case for the first block it can have the same timestamp as the previous block.
        assert!(prev_block.timestamp <= next_block.timestamp);
    } else {
        assert_eq!(prev_block.number + 1, next_block.number);
        if protocol_version.is_pre_fast_block() {
            assert!(prev_block.timestamp < next_block.timestamp);
        }
    }
    assert_eq!(prev_block.hash, next_block.prev_block_hash);
}

/// Returns the hash of the l2_block.
/// `txs_rolling_hash` of the l2_block is calculated the following way:
/// If the l2_block has 0 transactions, then `txs_rolling_hash` is equal to `H256::zero()`.
/// If the l2_block has i transactions, then `txs_rolling_hash` is equal to `H(H_{i-1}, H(tx_i))`, where
/// `H_{i-1}` is the `txs_rolling_hash` of the first i-1 transactions.
pub(crate) fn l2_block_hash(
    l2_block_number: L2BlockNumber,
    l2_block_timestamp: u64,
    prev_l2_block_hash: H256,
    txs_rolling_hash: H256,
) -> H256 {
    let mut digest: [u8; 128] = [0u8; 128];
    U256::from(l2_block_number.0).to_big_endian(&mut digest[0..32]);
    U256::from(l2_block_timestamp).to_big_endian(&mut digest[32..64]);
    digest[64..96].copy_from_slice(prev_l2_block_hash.as_bytes());
    digest[96..128].copy_from_slice(txs_rolling_hash.as_bytes());

    H256(keccak256(&digest))
}

/// Get last saved block from storage
pub fn load_last_l2_block<S: ReadStorage>(storage: &StoragePtr<S>) -> Option<L2Block> {
    // Get block number and timestamp
    let current_l2_block_info_key = StorageKey::new(
        AccountTreeId::new(SYSTEM_CONTEXT_ADDRESS),
        SYSTEM_CONTEXT_CURRENT_L2_BLOCK_INFO_POSITION,
    );
    let mut storage_ptr = storage.borrow_mut();
    let current_l2_block_info = storage_ptr.read_value(&current_l2_block_info_key);
    let (block_number, block_timestamp) = unpack_block_info(h256_to_u256(current_l2_block_info));
    let block_number = block_number as u32;
    if block_number == 0 {
        // The block does not exist yet
        return None;
    }

    // Get previous block hash
    let position = get_l2_block_hash_key(block_number - 1);
    let prev_block_hash = storage_ptr.read_value(&position);

    // Get current tx rolling hash
    let position = StorageKey::new(
        AccountTreeId::new(SYSTEM_CONTEXT_ADDRESS),
        SYSTEM_CONTEXT_CURRENT_TX_ROLLING_HASH_POSITION,
    );
    let current_tx_rolling_hash = storage_ptr.read_value(&position);

    // Calculate current hash
    let current_block_hash = l2_block_hash(
        L2BlockNumber(block_number),
        block_timestamp,
        prev_block_hash,
        current_tx_rolling_hash,
    );

    Some(L2Block {
        number: block_number,
        timestamp: block_timestamp,
        hash: current_block_hash,
    })
}
