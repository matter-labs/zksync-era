use crate::hasher::Hasher;
use zksync_basic_types::web3::signing::keccak256;

#[derive(Default, Clone, Debug)]
pub struct KeccakHasher;

impl Hasher<Vec<u8>> for KeccakHasher {
    /// Gets the hash of the byte sequence.
    fn hash_bytes<I: IntoIterator<Item = u8>>(&self, value: I) -> Vec<u8> {
        let value: Vec<u8> = value.into_iter().collect();
        keccak256(&value).to_vec()
    }

    /// Get the hash of the hashes sequence.
    fn hash_elements<I: IntoIterator<Item = Vec<u8>>>(&self, elements: I) -> Vec<u8> {
        let elems: Vec<u8> = elements.into_iter().flatten().collect();
        keccak256(&elems).to_vec()
    }

    /// Merges two hashes into one.
    fn compress(&self, lhs: &Vec<u8>, rhs: &Vec<u8>) -> Vec<u8> {
        let mut elems = vec![];
        elems.extend(lhs);
        elems.extend(rhs);

        keccak256(&elems).to_vec()
    }
}
