use blake2::{Blake2s256, Digest};
use zksync_basic_types::Address;
use zksync_types::{H256, U256};

pub mod json;

pub const SYSTEM_BLOCK_INFO_BLOCK_NUMBER_MULTIPLIER: U256 = U256([0, 0, 1, 0]);

pub fn h256_to_u256(num: H256) -> U256 {
    U256::from_big_endian(num.as_bytes())
}

/// Returns block.number/timestamp based on the block's information
pub fn unpack_block_info(info: U256) -> (u64, u64) {
    let block_number = (info / SYSTEM_BLOCK_INFO_BLOCK_NUMBER_MULTIPLIER).as_u64();
    let block_timestamp = (info % SYSTEM_BLOCK_INFO_BLOCK_NUMBER_MULTIPLIER).as_u64();
    (block_number, block_timestamp)
}

pub fn derive_final_address_for_params(address: &Address, key: &U256) -> [u8; 32] {
    let mut buffer = [0u8; 64];
    buffer[12..32].copy_from_slice(&address.0);
    key.to_big_endian(&mut buffer[32..64]);

    let mut result = [0u8; 32];
    result.copy_from_slice(Blake2s256::digest(buffer).as_slice());

    result
}
