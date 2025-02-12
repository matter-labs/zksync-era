use std::iter;

use once_cell::sync::Lazy;
use zksync_basic_types::H256;
use zksync_crypto_primitives::hasher::{blake2::Blake2Hasher, Hasher};

use crate::types::Leaf;

mod nodes;

/// Tree hashing functionality.
pub trait HashTree: Send + Sync {
    /// Returns the unique name of the hasher. This is used in Merkle tree tags to ensure
    /// that the tree remains consistent.
    fn name(&self) -> &'static str;

    /// Hashes a leaf node.
    fn hash_leaf(&self, leaf: &Leaf) -> H256;
    /// Compresses hashes in an intermediate node of a binary Merkle tree.
    fn hash_branch(&self, lhs: &H256, rhs: &H256) -> H256;

    /// Returns the hash of an empty subtree with the given depth. `depth == 0` corresponds to leaves. Implementations
    /// are encouraged to cache the returned values.
    ///
    /// Guaranteed to never be called with `depth > 64` (i.e., exceeding the depth of the entire tree).
    fn empty_subtree_hash(&self, depth: u8) -> H256;
}

impl HashTree for Blake2Hasher {
    fn name(&self) -> &'static str {
        "Blake2s256"
    }

    fn hash_leaf(&self, leaf: &Leaf) -> H256 {
        let mut hashed_bytes = [0; 2 * 32 + 2 * 8];
        hashed_bytes[..32].copy_from_slice(leaf.key.as_bytes());
        hashed_bytes[32..64].copy_from_slice(leaf.value.as_bytes());
        hashed_bytes[64..72].copy_from_slice(&leaf.prev_index.to_le_bytes());
        hashed_bytes[72..].copy_from_slice(&leaf.next_index.to_le_bytes());
        self.hash_bytes(&hashed_bytes)
    }

    fn hash_branch(&self, lhs: &H256, rhs: &H256) -> H256 {
        self.compress(lhs, rhs)
    }

    fn empty_subtree_hash(&self, depth: u8) -> H256 {
        static EMPTY_TREE_HASHES: Lazy<Vec<H256>> = Lazy::new(compute_empty_tree_hashes);
        EMPTY_TREE_HASHES[usize::from(depth)]
    }
}

fn compute_empty_tree_hashes() -> Vec<H256> {
    let empty_leaf_hash = Blake2Hasher.hash_bytes(&[0_u8; 2 * 32 + 2 * 8]);
    iter::successors(Some(empty_leaf_hash), |hash| {
        Some(Blake2Hasher.hash_branch(hash, hash))
    })
    .take(usize::from(Leaf::NIBBLES) * 4 + 1)
    .collect()
}
