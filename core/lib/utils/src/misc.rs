use zksync_basic_types::{web3::keccak256, H256, U256};

pub fn concat_and_hash(hash1: H256, hash2: H256) -> H256 {
    let mut bytes = [0_u8; 64];
    bytes[..32].copy_from_slice(&hash1.0);
    bytes[32..].copy_from_slice(&hash2.0);
    H256(keccak256(&bytes))
}

// FIXME: to commitment generator
pub fn expand_memory_contents(packed: &[(usize, U256)], memory_size_bytes: usize) -> Vec<u8> {
    let mut result: Vec<u8> = vec![0; memory_size_bytes];

    for (offset, value) in packed {
        value.to_big_endian(&mut result[(offset * 32)..(offset + 1) * 32]);
    }

    result
}
