use blake2::{Blake2s256, Digest};

use crate::hasher::Hasher;
use zksync_basic_types::H256;

#[derive(Default, Clone, Debug)]
pub struct Blake2Hasher;

impl Hasher for Blake2Hasher {
    type Hash = H256;

    fn hash_bytes(&self, value: &[u8]) -> H256 {
        let mut hasher = Blake2s256::new();
        hasher.update(value);
        H256(hasher.finalize().into())
    }

    fn compress(&self, lhs: &H256, rhs: &H256) -> H256 {
        let mut hasher = Blake2s256::new();
        hasher.update(lhs.as_ref());
        hasher.update(rhs.as_ref());
        H256(hasher.finalize().into())
    }
}
