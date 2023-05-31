use blake2::{Blake2s256, Digest};

use crate::hasher::Hasher;
use zksync_basic_types::H256;

#[derive(Default, Clone, Debug)]
pub struct Blake2Hasher;

impl Hasher<Vec<u8>> for Blake2Hasher {
    /// Gets the hash of the byte sequence.
    fn hash_bytes<I: IntoIterator<Item = u8>>(&self, value: I) -> Vec<u8> {
        <Self as Hasher<H256>>::hash_bytes(self, value).0.into()
    }

    /// Merges two hashes into one.
    fn compress(&self, lhs: &Vec<u8>, rhs: &Vec<u8>) -> Vec<u8> {
        let mut hasher = Blake2s256::new();
        hasher.update(lhs);
        hasher.update(rhs);
        hasher.finalize().to_vec()
    }
}

impl Hasher<H256> for Blake2Hasher {
    fn hash_bytes<I: IntoIterator<Item = u8>>(&self, value: I) -> H256 {
        let mut hasher = Blake2s256::new();
        let value: Vec<u8> = value.into_iter().collect();
        hasher.update(&value);
        H256(hasher.finalize().into())
    }

    fn compress(&self, lhs: &H256, rhs: &H256) -> H256 {
        let mut hasher = Blake2s256::new();
        hasher.update(lhs.as_ref());
        hasher.update(rhs.as_ref());
        H256(hasher.finalize().into())
    }
}
