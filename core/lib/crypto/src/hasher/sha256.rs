use sha2::{Digest, Sha256};

use crate::hasher::Hasher;
use zksync_basic_types::H256;

#[derive(Debug, Default, Clone, Copy)]
pub struct Sha256Hasher;

impl Hasher for Sha256Hasher {
    type Hash = H256;

    fn hash_bytes(&self, value: &[u8]) -> Self::Hash {
        let mut sha256 = Sha256::new();
        sha256.update(value);
        H256(sha256.finalize().into())
    }

    fn compress(&self, lhs: &Self::Hash, rhs: &Self::Hash) -> Self::Hash {
        let mut hasher = Sha256::new();
        hasher.update(lhs.as_ref());
        hasher.update(rhs.as_ref());
        H256(hasher.finalize().into())
    }
}
