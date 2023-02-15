pub mod blake2;
pub mod keccak;
pub mod sha256;

/// Definition of hasher suitable for calculating state hash.
///
/// # Panics
///
/// This structure expects input data to be correct, as it's main usage is the Merkle tree maintenance,
/// which assumes the consistent state.
/// It means that caller is responsible for checking that input values are actually valid, e.g. for `Vec<u8>`
/// it must be checked that byte sequence can be deserialized to hash object expected by the chosen hasher
/// implementation.
///
/// What it *actually* means, that is incorrect input data will cause the code to panic.
pub trait Hasher<Hash> {
    /// Gets the hash of the byte sequence.
    fn hash_bytes<I: IntoIterator<Item = u8>>(&self, value: I) -> Hash;
    /// Get the hash of the hashes sequence.
    fn hash_elements<I: IntoIterator<Item = Hash>>(&self, elements: I) -> Hash;
    /// Merges two hashes into one.
    fn compress(&self, lhs: &Hash, rhs: &Hash) -> Hash;
}
