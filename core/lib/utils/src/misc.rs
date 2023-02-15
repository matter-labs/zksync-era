use zksync_basic_types::web3::signing::keccak256;
use zksync_basic_types::{MiniblockNumber, H256, U256};

pub fn miniblock_hash(miniblock_number: MiniblockNumber) -> H256 {
    H256(keccak256(&miniblock_number.0.to_be_bytes()))
}

pub const fn ceil_div(a: u64, b: u64) -> u64 {
    (a + b - 1) / b
}

pub fn ceil_div_u256(a: U256, b: U256) -> U256 {
    (a + b - U256::from(1)) / b
}
