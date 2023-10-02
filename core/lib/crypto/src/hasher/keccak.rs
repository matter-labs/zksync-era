use crate::hasher::Hasher;
use zksync_basic_types::{web3::signing::keccak256, H256};

#[derive(Default, Clone, Debug)]
pub struct KeccakHasher;

impl Hasher for KeccakHasher {
    type Hash = H256;

    fn hash_bytes(&self, value: &[u8]) -> Self::Hash {
        H256(keccak256(value))
    }

    fn compress(&self, lhs: &Self::Hash, rhs: &Self::Hash) -> Self::Hash {
        let mut bytes = [0_u8; 64];
        bytes[..32].copy_from_slice(&lhs.0);
        bytes[32..].copy_from_slice(&rhs.0);
        H256(keccak256(&bytes))
    }
}
