use zksync_basic_types::u256_to_h256;
use zksync_crypto_primitives::hasher::{blake2::Blake2Hasher, Hasher};

use crate::{H256, U256};

#[derive(Debug, Clone, Copy)]
pub struct AccountProperties {
    pub versioning_data: u64,
    pub nonce: u64,
    pub observable_bytecode_hash: H256,
    pub bytecode_hash: H256,
    // TODO: rename to balance?
    pub nominal_token_balance: U256,
    pub bytecode_len: u32,
    pub artifacts_len: u32,
    pub observable_bytecode_len: u32,
}

impl AccountProperties {
    pub fn encode(&self) -> [u8; 124] {
        let mut result = Vec::new();
        result.extend_from_slice(&self.versioning_data.to_be_bytes());
        result.extend_from_slice(&self.nonce.to_be_bytes());
        result.extend_from_slice(u256_to_h256(self.nominal_token_balance).as_bytes());
        result.extend_from_slice(self.bytecode_hash.as_bytes());
        result.extend_from_slice(&self.bytecode_len.to_be_bytes());
        result.extend_from_slice(&self.artifacts_len.to_be_bytes());
        result.extend_from_slice(self.observable_bytecode_hash.as_bytes());
        result.extend_from_slice(&self.observable_bytecode_len.to_be_bytes());

        result.try_into().unwrap()
    }

    pub fn hash(&self) -> H256 {
        Blake2Hasher.hash_bytes(&self.encode())
    }
}
