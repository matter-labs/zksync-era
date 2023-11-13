//! Crate allowing to calculate root hashes and Merkle proofs for small in-memory Merkle trees.

// Linter settings.
#![warn(missing_debug_implementations, missing_docs, bare_trait_objects)]
#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::must_use_candidate, clippy::similar_names)]

use once_cell::sync::Lazy;

use std::{iter, str::FromStr};

#[cfg(test)]
mod tests;

use zksync_basic_types::H256;
use zksync_crypto::hasher::{keccak::KeccakHasher, Hasher};

/// Maximum supported depth of the tree. 32 corresponds to `2^32` elements in the tree, which
/// we unlikely to ever hit.
const MAX_TREE_DEPTH: usize = 32;

/// In-memory Merkle tree of bounded depth (no more than 10).
///
/// The tree is left-leaning, meaning that during its initialization, the size of a tree
/// can be specified larger than the number of provided leaves. In this case, the remaining leaves
/// will be considered to equal `[0_u8; LEAF_SIZE]`.
#[derive(Debug, Clone)]
pub struct MiniMerkleTree<const LEAF_SIZE: usize, H = KeccakHasher> {
    hasher: H,
    hashes: Box<[H256]>,
    binary_tree_size: usize,
}

impl<const LEAF_SIZE: usize> MiniMerkleTree<LEAF_SIZE>
where
    KeccakHasher: HashEmptySubtree<LEAF_SIZE>,
{
    /// Creates a new Merkle tree from the supplied leaves. If `min_tree_size` is supplied and is larger
    /// than the number of the supplied leaves, the leaves are padded to `min_tree_size` with `[0_u8; LEAF_SIZE]` entries.
    /// The hash function used in keccak-256.
    ///
    /// # Panics
    ///
    /// Panics in the same situations as [`Self::with_hasher()`].
    pub fn new(
        leaves: impl Iterator<Item = [u8; LEAF_SIZE]>,
        min_tree_size: Option<usize>,
    ) -> Self {
        Self::with_hasher(KeccakHasher, leaves, min_tree_size)
    }
}

impl<const LEAF_SIZE: usize, H> MiniMerkleTree<LEAF_SIZE, H>
where
    H: HashEmptySubtree<LEAF_SIZE>,
{
    /// Creates a new Merkle tree from the supplied leaves. If `min_tree_size` is supplied and is larger than the
    /// number of the supplied leaves, the leaves are padded to `min_tree_size` with `[0_u8; LEAF_SIZE]` entries.
    ///
    /// # Panics
    ///
    /// Panics if any of the following conditions applies:
    ///
    /// - `min_tree_size` (if supplied) is not a power of 2.
    pub fn with_hasher(
        hasher: H,
        leaves: impl Iterator<Item = [u8; LEAF_SIZE]>,
        min_tree_size: Option<usize>,
    ) -> Self {
        let hashes: Box<[H256]> = leaves.map(|bytes| hasher.hash_bytes(&bytes)).collect();
        let mut binary_tree_size = hashes.len().next_power_of_two();
        if let Some(min_tree_size) = min_tree_size {
            assert!(
                min_tree_size.is_power_of_two(),
                "tree size must be a power of 2"
            );
            binary_tree_size = min_tree_size.max(binary_tree_size);
        }
        assert!(
            tree_depth_by_size(binary_tree_size) <= MAX_TREE_DEPTH,
            "Tree contains more than {} items; this is not supported",
            1 << MAX_TREE_DEPTH
        );

        Self {
            hasher,
            hashes,
            binary_tree_size,
        }
    }

    /// Returns the root hash of this tree.
    /// # Panics
    /// Will panic if the constant below is invalid.
    pub fn merkle_root(self) -> H256 {
        if self.hashes.is_empty() {
            H256::from_str("fef7bd9f889811e59e4076a0174087135f080177302763019adaf531257e3a87")
                .unwrap()
        } else {
            self.compute_merkle_root_and_path(0, None)
        }
    }

    /// Returns the root hash and the Merkle proof for a leaf with the specified 0-based `index`.
    pub fn merkle_root_and_path(self, index: usize) -> (H256, Vec<H256>) {
        let mut merkle_path = vec![];
        let root_hash = self.compute_merkle_root_and_path(index, Some(&mut merkle_path));
        (root_hash, merkle_path)
    }

    fn compute_merkle_root_and_path(
        self,
        mut index: usize,
        mut merkle_path: Option<&mut Vec<H256>>,
    ) -> H256 {
        assert!(index < self.hashes.len(), "invalid tree leaf index");

        let depth = tree_depth_by_size(self.binary_tree_size);
        if let Some(merkle_path) = merkle_path.as_deref_mut() {
            merkle_path.reserve(depth);
        }

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
pub trait HashEmptySubtree<const LEAF_SIZE: usize>:
    'static + Send + Sync + Hasher<Hash = H256>
{
    /// Returns the hash of an empty subtree with the given depth. Implementations
    /// are encouraged to cache the returned values.
    fn empty_subtree_hash(&self, depth: usize) -> H256;
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
