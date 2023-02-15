use crate::hasher::Hasher;
use blake2::{Blake2s256, Digest};

#[derive(Default, Clone, Debug)]
pub struct Blake2Hasher;

impl Hasher<Vec<u8>> for Blake2Hasher {
    /// Gets the hash of the byte sequence.
    fn hash_bytes<I: IntoIterator<Item = u8>>(&self, value: I) -> Vec<u8> {
        let mut hasher = Blake2s256::new();
        let value: Vec<u8> = value.into_iter().collect();
        hasher.update(&value);

        hasher.finalize().to_vec()
    }

    /// Get the hash of the hashes sequence.
    fn hash_elements<I: IntoIterator<Item = Vec<u8>>>(&self, elements: I) -> Vec<u8> {
        let elems: Vec<u8> = elements.into_iter().flatten().collect();

        let mut hasher = Blake2s256::new();
        hasher.update(&elems);
        hasher.finalize().to_vec()
    }

    /// Merges two hashes into one.
    fn compress(&self, lhs: &Vec<u8>, rhs: &Vec<u8>) -> Vec<u8> {
        let mut elems = vec![];
        elems.extend(lhs);
        elems.extend(rhs);

        let mut hasher = Blake2s256::new();
        hasher.update(&elems);
        hasher.finalize().to_vec()
    }
}
