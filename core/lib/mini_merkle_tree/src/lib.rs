//! Crate allowing to calculate root hashes and Merkle proofs for small in-memory Merkle trees.

// Linter settings.
#![warn(missing_debug_implementations, missing_docs, bare_trait_objects)]
#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::must_use_candidate, clippy::similar_names)]

use std::{collections::VecDeque, iter, marker::PhantomData, ops::RangeTo};

#[cfg(test)]
mod tests;

use zksync_basic_types::H256;
use zksync_crypto_primitives::hasher::{keccak::KeccakHasher, Hasher};

/// Maximum supported depth of the tree. 32 corresponds to `2^32` elements in the tree, which
/// we unlikely to ever hit.
const MAX_TREE_DEPTH: usize = 32;

/// In-memory Merkle tree of bounded depth (no more than 32).
///
/// The tree is left-leaning, meaning that during its initialization, the size of a tree
/// can be specified larger than the number of provided leaves. In this case, the remaining leaves
/// will be considered to equal `[0_u8; LEAF_SIZE]`.
///
/// The tree has dynamic size, meaning that it can grow by a factor of 2 when the number of leaves
/// exceeds the current tree size. It does not shrink.
///
/// The tree is optimized for the case when the queries are performed on the rightmost leaves
/// and the leftmost leaves are cached (trimmed). Caching enables the merkle roots and paths to be computed
/// in `O(max(n, depth))` time, where `n` is the number of uncached leaves (in contrast to the total number of
/// leaves). Cache itself only takes up `O(depth)` space. However, caching prevents the retrieval of paths to the
/// cached leaves.
#[derive(Debug, Clone)]
pub struct MiniMerkleTree<L, H = KeccakHasher> {
    hasher: H,
    /// Stores untrimmed (uncached) leaves of the tree.
    hashes: VecDeque<H256>,
    /// Size of the tree. Always a power of 2.
    /// If it is greater than `self.start_index + self.hashes.len()`, the remaining leaves are empty.
    binary_tree_size: usize,
    /// Index of the leftmost untrimmed leaf.
    start_index: usize,
    /// Left subset of the Merkle path to the first untrimmed leaf (i.e., a leaf with index `self.start_index`).
    /// Merkle path starts from the bottom of the tree and goes up.
    /// Used to fill in data for trimmed tree leaves when computing Merkle paths and the root hash.
    /// Because only the left subset of the path is used, the cache is not invalidated when new leaves are
    /// pushed into the tree. If all leaves are trimmed, cache is the left subset of the Merkle path to
    /// the next leaf to be inserted, which still has index `self.start_index`.
    cache: Vec<Option<H256>>,
    /// Leaf type marker
    _leaf: PhantomData<L>,
    /// empty leaf hash
    empty_leaf_hash: Option<H256>,
}

impl<L: AsRef<[u8]>> MiniMerkleTree<L>
where
    KeccakHasher: HashEmptySubtree<L>,
{
    /// Creates a new Merkle tree from the supplied leaves. If `min_tree_size` is supplied and is larger
    /// than the number of the supplied leaves, the leaves are padded to `min_tree_size` with `[0_u8; LEAF_SIZE]` entries.
    /// The hash function used in keccak-256.
    ///
    /// # Panics
    ///
    /// Panics in the same situations as [`Self::with_hasher()`].
    pub fn new(leaves: impl Iterator<Item = L>, min_tree_size: Option<usize>) -> Self {
        Self::with_hasher(KeccakHasher, leaves, min_tree_size)
    }

    /// Same as above just sets empty leaf hash
    pub fn new_with_empty_leaf_hash(
        leaves: impl Iterator<Item = L>,
        min_tree_size: Option<usize>,
        empty_leaf_hash: H256,
    ) -> Self {
        let mut tree = Self::with_hasher(KeccakHasher, leaves, min_tree_size);
        tree.empty_leaf_hash = Some(empty_leaf_hash);
        tree
    }
}

impl<L: AsRef<[u8]>, H> MiniMerkleTree<L, H>
where
    H: HashEmptySubtree<L>,
{
    /// Creates a new Merkle tree from the supplied leaves. If `min_tree_size` is supplied and is larger than the
    /// number of the supplied leaves, the leaves are padded to `min_tree_size` with `[0_u8; LEAF_SIZE]` entries,
    /// but are deemed empty.
    ///
    /// # Panics
    ///
    /// Panics if any of the following conditions applies:
    ///
    /// - `min_tree_size` (if supplied) is not a power of 2.
    /// - The number of leaves is greater than `2^32`.
    pub fn with_hasher(
        hasher: H,
        leaves: impl Iterator<Item = L>,
        min_tree_size: Option<usize>,
    ) -> Self {
        let hashes: Vec<_> = leaves
            .map(|bytes| hasher.hash_bytes(bytes.as_ref()))
            .collect();
        Self::from_hashes(hasher, hashes.into_iter(), min_tree_size)
    }

    /// Adds a new leaf to the tree (replaces leftmost empty leaf).
    /// If the tree is full, its size is doubled.
    /// Note: empty leaves != zero leaves.
    pub fn push(&mut self, leaf: L) {
        let leaf_hash = self.hasher.hash_bytes(leaf.as_ref());
        self.push_hash(leaf_hash);
    }
}

impl<L, H> MiniMerkleTree<L, H>
where
    H: HashEmptySubtree<L>,
{
    /// Creates a new Merkle tree from the supplied raw hashes. If `min_tree_size` is supplied and is larger than the
    /// number of the supplied leaves, the leaves are padded to `min_tree_size` with zero-hash entries,
    /// but are deemed empty.
    ///
    /// # Panics
    ///
    /// Panics if any of the following conditions applies:
    ///
    /// - `min_tree_size` (if supplied) is not a power of 2.
    /// - The number of leaves is greater than `2^32`.
    pub fn from_hashes(
        hasher: H,
        hashes: impl Iterator<Item = H256>,
        min_tree_size: Option<usize>,
    ) -> Self {
        let hashes: VecDeque<_> = hashes.collect();
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
            1u64 << MAX_TREE_DEPTH
        );

        Self {
            hasher,
            hashes,
            binary_tree_size,
            start_index: 0,
            cache: vec![None; depth],
            _leaf: PhantomData,
            empty_leaf_hash: None,
        }
    }

    /// Returns `true` if the tree is empty.
    pub fn is_empty(&self) -> bool {
        self.start_index == 0 && self.hashes.is_empty()
    }

    /// Returns the root hash of this tree.
    #[allow(clippy::missing_panics_doc)] // Should never panic, unless there is a bug.
    pub fn merkle_root(&self) -> H256 {
        if self.hashes.is_empty() {
            let depth = tree_depth_by_size(self.binary_tree_size);
            if self.start_index == 0 {
                return self.hasher.empty_subtree_hash(depth);
            } else if self.start_index == self.binary_tree_size {
                return self.cache[depth].expect("cache is invalid");
            }
        }
        self.compute_merkle_root_and_path(0, None, None)
    }

    /// Returns the root hash and the Merkle proof for a leaf with the specified 0-based `index`.
    /// `index` is relative to the leftmost uncached leaf.
    /// # Panics
    /// Panics if `index` is >= than the number of uncached leaves in the tree.
    pub fn merkle_root_and_path(&self, index: usize) -> (H256, Vec<H256>) {
        assert!(index < self.hashes.len(), "leaf index out of bounds");
        let mut end_path = vec![];
        let root_hash = self.compute_merkle_root_and_path(index, Some(&mut end_path), None);
        (
            root_hash,
            end_path.into_iter().map(Option::unwrap).collect(),
        )
    }

    /// Returns the root hash and the Merkle proof for a leaf with the specified 0-based `index`.
    /// `index` is an absolute position of the leaf.
    /// # Panics
    /// Panics if leaf at `index` is cached or if `index` is >= than the number of leaves in the tree.
    pub fn merkle_root_and_path_by_absolute_index(&self, index: usize) -> (H256, Vec<H256>) {
        assert!(index >= self.start_index, "leaf is cached");
        self.merkle_root_and_path(index - self.start_index)
    }

    /// Returns the root hash and the Merkle proofs for a range of leafs.
    /// The range is 0..length, where `0` is the leftmost untrimmed leaf (i.e. leaf under `self.start_index`).
    /// # Panics
    /// Panics if `range.end` is 0 or greater than the number of leaves in the tree.
    pub fn merkle_root_and_paths_for_range(
        &self,
        range: RangeTo<usize>,
    ) -> (H256, Vec<Option<H256>>, Vec<Option<H256>>) {
        assert!(range.end > 0, "empty range");
        assert!(range.end <= self.hashes.len(), "range index out of bounds");
        let mut right_path = vec![];
        let root_hash = self.compute_merkle_root_and_path(
            range.end - 1,
            Some(&mut right_path),
            Some(Side::Right),
        );
        (root_hash, self.cache.clone(), right_path)
    }

    /// Adds a raw hash to the tree (replaces leftmost empty leaf).
    /// If the tree is full, its size is doubled.
    /// Note: empty leaves != zero leaves.
    pub fn push_hash(&mut self, leaf_hash: H256) {
        self.hashes.push_back(leaf_hash);
        if self.start_index + self.hashes.len() > self.binary_tree_size {
            self.binary_tree_size *= 2;
            if self.cache.len() < tree_depth_by_size(self.binary_tree_size) {
                self.cache.push(None);
            }
        }
    }

    /// Returns the leftmost `length` untrimmed leaf hashes.
    pub fn hashes_prefix(&self, length: usize) -> Vec<H256> {
        self.hashes.iter().take(length).copied().collect()
    }

    /// Trims and caches the leftmost `count` leaves.
    /// Does not affect the root hash, but makes it impossible to get the paths to the cached leaves.
    /// # Panics
    /// Panics if `count` is greater than the number of untrimmed leaves in the tree.
    pub fn trim_start(&mut self, count: usize) {
        assert!(self.hashes.len() >= count, "not enough leaves to trim");
        let mut new_cache = vec![];
        // Cache is a left subset of the path to the first untrimmed leaf.
        let root = self.compute_merkle_root_and_path(count, Some(&mut new_cache), Some(Side::Left));
        self.hashes.drain(..count);
        self.start_index += count;
        if self.start_index == self.binary_tree_size {
            // If the tree is completely trimmed *and* will grow on the next push,
            // we need to cache the root.
            new_cache.push(Some(root));
        }
        self.cache = new_cache;
    }

    /// Computes the Merkle root hash.
    /// If `path` is `Some`, also computes the Merkle path to the leaf with the specified
    /// `index` (relative to `self.start_index`).
    /// If `side` is `Some`, only the corresponding side subset of the path is computed
    /// (`Some` for elements in the `side` subset of the path, `None` for the other elements).
    fn compute_merkle_root_and_path(
        &self,
        mut index: usize,
        mut path: Option<&mut Vec<Option<H256>>>,
        side: Option<Side>,
    ) -> H256 {
        let depth = tree_depth_by_size(self.binary_tree_size);
        if let Some(path) = path.as_deref_mut() {
            path.reserve(depth);
        }

        let mut hashes = self.hashes.clone();
        let mut absolute_start_index = self.start_index;

        for level in 0..depth {
            // If the first untrimmed leaf is a right sibling,
            // add it's left sibling to `hashes` from cache for convenient iteration later.
            if absolute_start_index % 2 == 1 {
                hashes.push_front(self.cache[level].expect("cache is invalid"));
                index += 1;
            }
            // At this point `hashes` always starts from the left sibling node.
            // If it ends on the left sibling node, add the right sibling node to `hashes`
            // for convenient iteration later.
            if hashes.len() % 2 == 1 {
                hashes.push_back(
                    compute_empty_tree_hashes(
                        self.empty_leaf_hash
                            .unwrap_or(self.hasher.empty_leaf_hash()),
                    )[level],
                );
            }

            if let Some(path) = path.as_deref_mut() {
                let hash = match side {
                    Some(Side::Left) if index % 2 == 0 => None,
                    Some(Side::Right) if index % 2 == 1 => None,
                    _ => hashes.get(index ^ 1).copied(),
                };
                path.push(hash);
            }

            let level_len = hashes.len() / 2;
            // Since `hashes` has an even number of elements, we can simply iterate over the pairs.
            for i in 0..level_len {
                hashes[i] = self.hasher.compress(&hashes[2 * i], &hashes[2 * i + 1]);
            }

            hashes.truncate(level_len);
            index /= 2;
            absolute_start_index /= 2;
        }

        hashes[0]
    }

    /// Returns the number of non-empty merkle tree elements.
    pub fn length(&self) -> usize {
        self.start_index + self.hashes.len()
    }

    /// Returns index of the leftmost untrimmed leaf.
    pub fn start_index(&self) -> usize {
        self.start_index
    }
}

fn tree_depth_by_size(tree_size: usize) -> usize {
    debug_assert!(tree_size.is_power_of_two());
    tree_size.trailing_zeros() as usize
}

/// Used to represent subsets of a Merkle path.
/// `Left` are the left sibling nodes, `Right` are the right sibling nodes.
#[derive(Debug, Clone, Copy)]
enum Side {
    Left,
    Right,
}

/// Hashing of empty binary Merkle trees.
pub trait HashEmptySubtree<L>: 'static + Send + Sync + Hasher<Hash = H256> {
    /// Returns the hash of an empty subtree with the given depth.
    /// Implementations are encouraged to cache the returned values.
    fn empty_subtree_hash(&self, depth: usize) -> H256 {
        // We do not cache by default since then the cached values would be preserved
        // for all implementations which is not correct for different leaves.
        compute_empty_tree_hashes(self.empty_leaf_hash())[depth]
    }

    /// Returns an empty hash
    fn empty_leaf_hash(&self) -> H256;
}

impl HashEmptySubtree<[u8; 88]> for KeccakHasher {
    fn empty_leaf_hash(&self) -> H256 {
        self.hash_bytes(&[0_u8; 88])
    }
}

impl HashEmptySubtree<[u8; 96]> for KeccakHasher {
    fn empty_leaf_hash(&self) -> H256 {
        self.hash_bytes(&[0_u8; 96])
    }
}

fn compute_empty_tree_hashes(empty_leaf_hash: H256) -> Vec<H256> {
    iter::successors(Some(empty_leaf_hash), |hash| {
        Some(KeccakHasher.compress(hash, hash))
    })
    .take(MAX_TREE_DEPTH + 1)
    .collect()
}
