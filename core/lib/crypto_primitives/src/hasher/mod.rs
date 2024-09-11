pub mod blake2;
pub mod keccak;
pub mod sha256;

/// Definition of hasher suitable for calculating state hash.
pub trait Hasher {
    type Hash: AsRef<[u8]>;

    /// Gets the hash of the byte sequence.
    fn hash_bytes(&self, value: &[u8]) -> Self::Hash;

    /// Merges two hashes into one.
    fn compress(&self, lhs: &Self::Hash, rhs: &Self::Hash) -> Self::Hash;
}
