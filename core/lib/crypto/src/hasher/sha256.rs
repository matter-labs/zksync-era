use crate::hasher::Hasher;
use sha2::{Digest, Sha256};

#[derive(Default, Clone, Debug)]
pub struct Sha256Hasher;

impl Hasher<Vec<u8>> for Sha256Hasher {
    /// Gets the hash of the byte sequence.
    fn hash_bytes<I: IntoIterator<Item = u8>>(&self, value: I) -> Vec<u8> {
        let mut sha256 = Sha256::new();
        let value: Vec<u8> = value.into_iter().collect();
        sha256.update(&value);

        sha256.finalize().to_vec()
    }

    /// Get the hash of the hashes sequence.
    fn hash_elements<I: IntoIterator<Item = Vec<u8>>>(&self, elements: I) -> Vec<u8> {
        let elems: Vec<u8> = elements.into_iter().flatten().collect();

        let mut sha256 = Sha256::new();
        sha256.update(&elems);
        sha256.finalize().to_vec()
    }

    /// Merges two hashes into one.
    fn compress(&self, lhs: &Vec<u8>, rhs: &Vec<u8>) -> Vec<u8> {
        let mut elems = vec![];
        elems.extend(lhs);
        elems.extend(rhs);

        let mut sha256 = Sha256::new();
        sha256.update(&elems);
        sha256.finalize().to_vec()
    }
}
