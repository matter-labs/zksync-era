//! Crate allowing to calculate root hashes and Merkle proofs for small in-memory Merkle trees.

// Linter settings.
#![warn(missing_debug_implementations, missing_docs, bare_trait_objects)]
#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::must_use_candidate, clippy::similar_names)]

use once_cell::sync::Lazy;

use std::{fmt, iter};

#[cfg(test)]
mod tests;

use zksync_basic_types::H256;
use zksync_crypto::hasher::{keccak::KeccakHasher, Hasher};

/// Maximum supported depth of Merkle trees. 10 means that the tree must have <=1,024 leaves.
const MAX_TREE_DEPTH: usize = 10;

/// In-memory Merkle tree of bounded depth (no more than 10).
///
/// The tree is left-leaning, meaning that during its initialization, the size of a tree
/// can be specified larger than the number of provided leaves. In this case, the remaining leaves
/// will be considered to equal `[0_u8; LEAF_SIZE]`.
#[derive(Debug, Clone)]
pub struct MiniMerkleTree<'a, const LEAF_SIZE: usize> {
    hasher: &'a dyn HashEmptySubtree<LEAF_SIZE>,
    hashes: Box<[H256]>,
    tree_size: usize,
}

impl<const LEAF_SIZE: usize> MiniMerkleTree<'static, LEAF_SIZE>
where
    KeccakHasher: HashEmptySubtree<LEAF_SIZE>,
{
    /// Creates a new Merkle tree from the supplied leaves. If `tree_size` is larger than the
    /// number of the supplied leaves, the remaining leaves are `[0_u8; LEAF_SIZE]`.
    /// The hash function used in keccak-256.
    ///
    /// # Panics
    ///
    /// Panics in the same situations as [`Self::with_hasher()`].
    pub fn new(leaves: impl Iterator<Item = [u8; LEAF_SIZE]>, tree_size: usize) -> Self {
        Self::with_hasher(&KeccakHasher, leaves, tree_size)
    }
}

impl<'a, const LEAF_SIZE: usize> MiniMerkleTree<'a, LEAF_SIZE> {
    /// Creates a new Merkle tree from the supplied leaves. If `tree_size` is larger than the
    /// number of the supplied leaves, the remaining leaves are `[0_u8; LEAF_SIZE]`.
    ///
    /// # Panics
    ///
    /// Panics if any of the following conditions applies:
    ///
    /// - The number of `leaves` is greater than `tree_size`.
    /// - `tree_size > 1_024`.
    /// - `tree_size` is not a power of 2.
    pub fn with_hasher(
        hasher: &'a dyn HashEmptySubtree<LEAF_SIZE>,
        leaves: impl Iterator<Item = [u8; LEAF_SIZE]>,
        tree_size: usize,
    ) -> Self {
        assert!(
            tree_size <= 1 << MAX_TREE_DEPTH,
            "tree size must be <={}",
            1 << MAX_TREE_DEPTH
        );
        assert!(
            tree_size.is_power_of_two(),
            "tree size must be a power of 2"
        );

        let hashes: Box<[H256]> = leaves.map(|bytes| hasher.hash_bytes(&bytes)).collect();
        assert!(
            hashes.len() <= tree_size,
            "tree size must be greater or equal the number of supplied leaves"
        );

        Self {
            hasher,
            hashes,
            tree_size,
        }
    }

    /// Returns the root hash of this tree.
    pub fn merkle_root(self) -> H256 {
        if self.hashes.is_empty() {
            H256::zero()
        } else {
            self.compute_merkle_root_and_path(0, None)
        }
    }

    /// Returns the root hash and the Merkle proof for a leaf with the specified 0-based `index`.
    pub fn merkle_root_and_path(self, index: usize) -> (H256, Vec<H256>) {
        let mut merkle_path = Vec::with_capacity(MAX_TREE_DEPTH);
        let root_hash = self.compute_merkle_root_and_path(index, Some(&mut merkle_path));
        (root_hash, merkle_path)
    }

    fn compute_merkle_root_and_path(
        self,
        mut index: usize,
        mut merkle_path: Option<&mut Vec<H256>>,
    ) -> H256 {
        assert!(index < self.hashes.len(), "invalid tree leaf index");

        let depth = tree_depth_by_size(self.tree_size);

        let mut hashes = self.hashes;
        let mut level_len = hashes.len();
        for level in 0..depth {
            let empty_hash_at_level = self.hasher.empty_subtree_hash(level);

            if let Some(merkle_path) = merkle_path.as_deref_mut() {
                let adjacent_idx = index ^ 1;
                let adjacent_hash = if adjacent_idx < level_len {
                    hashes[adjacent_idx]
                } else {
                    empty_hash_at_level
                };
                merkle_path.push(adjacent_hash);
            }

            for i in 0..(level_len / 2) {
                hashes[i] = self.hasher.compress(&hashes[2 * i], &hashes[2 * i + 1]);
            }
            if level_len % 2 == 1 {
                hashes[level_len / 2] = self
                    .hasher
                    .compress(&hashes[level_len - 1], &empty_hash_at_level);
            }

            index /= 2;
            level_len = level_len / 2 + level_len % 2;
        }
        hashes[0]
    }
}

fn tree_depth_by_size(tree_size: usize) -> usize {
    debug_assert!(tree_size.is_power_of_two());
    tree_size.trailing_zeros() as usize
}

/// Hashing of empty binary Merkle trees.
pub trait HashEmptySubtree<const LEAF_SIZE: usize>: Hasher<Hash = H256> {
    /// Returns the hash of an empty subtree with the given depth. Implementations
    /// are encouraged to cache the returned values.
    fn empty_subtree_hash(&self, depth: usize) -> H256;
}

impl<const LEAF_SIZE: usize> fmt::Debug for dyn HashEmptySubtree<LEAF_SIZE> + '_ {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HashTree")
            .field("LEAF_SIZE", &LEAF_SIZE)
            .finish()
    }
}

impl HashEmptySubtree<88> for KeccakHasher {
    fn empty_subtree_hash(&self, depth: usize) -> H256 {
        static EMPTY_TREE_HASHES: Lazy<Vec<H256>> = Lazy::new(compute_empty_tree_hashes::<88>);
        EMPTY_TREE_HASHES[depth]
    }
}

fn compute_empty_tree_hashes<const LEAF_SIZE: usize>() -> Vec<H256> {
    let empty_leaf_hash = KeccakHasher.hash_bytes(&[0_u8; LEAF_SIZE]);
    iter::successors(Some(empty_leaf_hash), |hash| {
        Some(KeccakHasher.compress(hash, hash))
    })
    .take(MAX_TREE_DEPTH + 1)
    .collect()
}
