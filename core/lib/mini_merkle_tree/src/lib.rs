//! Crate allowing to calculate root hashes and Merkle proofs for small in-memory Merkle trees.

// Linter settings.
#![warn(missing_debug_implementations, missing_docs, bare_trait_objects)]
#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::must_use_candidate, clippy::similar_names)]

use std::{collections::VecDeque, iter};

use once_cell::sync::Lazy;

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
    hashes: VecDeque<H256>,
    binary_tree_size: usize,
    head_index: usize,
    left_cache: Vec<H256>,
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
        let hashes: VecDeque<_> = leaves.map(|bytes| hasher.hash_bytes(&bytes)).collect();
        let mut binary_tree_size = hashes.len().next_power_of_two();
        if let Some(min_tree_size) = min_tree_size {
            assert!(
                min_tree_size.is_power_of_two(),
                "tree size must be a power of 2"
            );
            binary_tree_size = min_tree_size.max(binary_tree_size);
        }
        let depth = tree_depth_by_size(binary_tree_size);
        assert!(
            depth <= MAX_TREE_DEPTH,
            "Tree contains more than {} items; this is not supported",
            1 << MAX_TREE_DEPTH
        );

        Self {
            hasher,
            hashes,
            binary_tree_size,
            head_index: 0,
            left_cache: vec![H256::default(); depth + 1],
        }
    }

    /// Returns the root hash of this tree.
    /// # Panics
    /// Will panic if the constant below is invalid.
    pub fn merkle_root(&self) -> H256 {
        if self.hashes.is_empty() {
            let depth = tree_depth_by_size(self.binary_tree_size);
            self.hasher.empty_subtree_hash(depth)
        } else {
            self.compute_merkle_root_and_path(0, None, None)
        }
    }

    /// Returns the root hash and the Merkle proof for a leaf with the specified 0-based `index`.
    pub fn merkle_root_and_path(&self, index: usize) -> (H256, Vec<H256>) {
        let (mut left_path, mut right_path) = (vec![], vec![]);
        let root_hash =
            self.compute_merkle_root_and_path(index, Some((&mut left_path, &mut right_path)), None);
        (root_hash, right_path)
    }

    /// Returns the root hash and the Merkle proof for an interval of leafs.
    /// The interval is [0, `length`), where `0` is the leftmost uncached leaf.
    pub fn merkle_root_and_paths_for_interval(
        &self,
        length: usize,
    ) -> (H256, Vec<H256>, Vec<H256>) {
        let (mut left_path, mut right_path) = (vec![], vec![]);
        let root_hash = self.compute_merkle_root_and_path(
            length - 1,
            Some((&mut left_path, &mut right_path)),
            None,
        );
        (root_hash, left_path, right_path)
    }

    /// Adds a new leaf to the tree (replaces leftmost empty leaf).
    /// If the tree is full, its size is doubled.
    /// Note: empty leaves != zero leaves.
    pub fn push(&mut self, leaf: [u8; LEAF_SIZE]) {
        let leaf_hash = self.hasher.hash_bytes(&leaf);
        self.hashes.push_back(leaf_hash);
        if self.head_index + self.hashes.len() > self.binary_tree_size {
            self.binary_tree_size *= 2;
            self.left_cache.push(H256::default());
        }
    }

    /// Caches the rightmost `count` leaves.
    /// Does not affect the root hash, but makes it impossible to get the paths to the cached leaves.
    ///
    /// # Panics
    ///
    /// Panics if `count` is greater than the number of non-cached leaves in the tree.
    pub fn cache(&mut self, count: usize) {
        assert!(self.hashes.len() >= count, "not enough leaves to cache");
        let depth = tree_depth_by_size(self.binary_tree_size);
        let mut new_cache = vec![H256::default(); depth + 1];
        self.compute_merkle_root_and_path(count - 1, None, Some(&mut new_cache));
        self.hashes.drain(0..count);
        self.head_index += count;
        debug_assert!(self.left_cache.len() == new_cache.len());
        self.left_cache = new_cache;
    }

    fn compute_merkle_root_and_path(
        &self,
        mut right_index: usize,
        mut merkle_paths: Option<(&mut Vec<H256>, &mut Vec<H256>)>,
        mut new_cache: Option<&mut Vec<H256>>,
    ) -> H256 {
        assert!(right_index < self.hashes.len(), "invalid tree leaf index");

        let depth = tree_depth_by_size(self.binary_tree_size);
        if let Some((left_path, right_path)) = &mut merkle_paths {
            left_path.reserve(depth);
            right_path.reserve(depth);
        }

        let mut hashes = self.hashes.clone();
        let mut level_len = hashes.len();
        let mut head_index = self.head_index;

        for level in 0..depth {
            let empty_hash_at_level = self.hasher.empty_subtree_hash(level);

            let sibling_hash = |index: usize| {
                if index == 0 && head_index % 2 == 1 {
                    self.left_cache[level]
                } else if index == level_len - 1 && (head_index + index) % 2 == 0 {
                    empty_hash_at_level
                } else {
                    let sibling = ((head_index + index) ^ 1) - head_index;
                    hashes[sibling]
                }
            };

            if let Some((left_path, right_path)) = &mut merkle_paths {
                left_path.push(sibling_hash(0));
                right_path.push(sibling_hash(right_index));
            }

            if let Some(new_cache) = new_cache.as_deref_mut() {
                new_cache[level] = if (head_index + right_index) % 2 == 0 {
                    hashes[right_index]
                } else if right_index == 0 {
                    self.left_cache[level]
                } else {
                    hashes[right_index - 1]
                };
            }

            let parity = head_index % 2;
            let next_level_len = level_len / 2 + (parity | (level_len % 2));

            for i in 0..next_level_len {
                let lhs = if i == 0 && parity == 1 {
                    self.left_cache[level]
                } else {
                    hashes[2 * i - parity]
                };
                let rhs = if i == next_level_len - 1 && (level_len - parity) % 2 == 1 {
                    empty_hash_at_level
                } else {
                    hashes[2 * i + 1 - parity]
                };
                hashes[i] = self.hasher.compress(&lhs, &rhs);
            }

            right_index = (right_index + parity) / 2;
            level_len = next_level_len;
            head_index /= 2;
        }

        if let Some(new_cache) = new_cache {
            // It is important to cache the root as well, in case
            // we just cached all elements and will grow on the next push.
            new_cache[depth] = hashes[0];
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
