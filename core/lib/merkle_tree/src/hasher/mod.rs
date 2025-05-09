//! Hashing operations on the Merkle tree.

use std::{fmt, iter, sync::LazyLock};

use zksync_crypto_primitives::hasher::{blake2::Blake2Hasher, Hasher};

pub(crate) use self::nodes::{InternalNodeCache, MerklePath};
pub use self::proofs::TreeRangeDigest;
use crate::{
    metrics::HashingStats,
    types::{TreeEntry, ValueHash, TREE_DEPTH},
};

mod nodes;
mod proofs;

/// Tree hashing functionality.
pub trait HashTree: Send + Sync {
    /// Returns the unique name of the hasher. This is used in Merkle tree tags to ensure
    /// that the tree remains consistent.
    fn name(&self) -> &'static str;

    /// Hashes a leaf node.
    fn hash_leaf(&self, value_hash: &ValueHash, leaf_index: u64) -> ValueHash;
    /// Compresses hashes in an intermediate node of a binary Merkle tree.
    fn hash_branch(&self, lhs: &ValueHash, rhs: &ValueHash) -> ValueHash;

    /// Returns the hash of an empty subtree with the given depth. Implementations
    /// are encouraged to cache the returned values.
    fn empty_subtree_hash(&self, depth: usize) -> ValueHash;

    /// Returns the hash of the empty tree. The default implementation uses [`Self::empty_subtree_hash()`].
    fn empty_tree_hash(&self) -> ValueHash {
        self.empty_subtree_hash(TREE_DEPTH)
    }
}

impl<H: HashTree + ?Sized> HashTree for &H {
    fn name(&self) -> &'static str {
        (**self).name()
    }

    fn hash_leaf(&self, value_hash: &ValueHash, leaf_index: u64) -> ValueHash {
        (**self).hash_leaf(value_hash, leaf_index)
    }

    fn hash_branch(&self, lhs: &ValueHash, rhs: &ValueHash) -> ValueHash {
        (**self).hash_branch(lhs, rhs)
    }

    fn empty_subtree_hash(&self, depth: usize) -> ValueHash {
        (**self).empty_subtree_hash(depth)
    }
}

impl dyn HashTree + '_ {
    /// Extends the provided `path` to length `TREE_DEPTH`.
    fn extend_merkle_path<'a>(
        &'a self,
        path: &'a [ValueHash],
    ) -> impl Iterator<Item = ValueHash> + 'a {
        let empty_hash_count = TREE_DEPTH - path.len();
        let empty_hashes = (0..empty_hash_count).map(|depth| self.empty_subtree_hash(depth));
        empty_hashes.chain(path.iter().copied())
    }

    fn fold_merkle_path(&self, path: &[ValueHash], entry: TreeEntry) -> ValueHash {
        let mut hash = self.hash_leaf(&entry.value, entry.leaf_index);
        let full_path = self.extend_merkle_path(path);
        for (depth, adjacent_hash) in full_path.enumerate() {
            hash = if entry.key.bit(depth) {
                self.hash_branch(&adjacent_hash, &hash)
            } else {
                self.hash_branch(&hash, &adjacent_hash)
            };
        }
        hash
    }

    pub(crate) fn with_stats<'a>(&'a self, stats: &'a HashingStats) -> HasherWithStats<'a> {
        HasherWithStats {
            shared_metrics: Some(stats),
            ..HasherWithStats::new(self)
        }
    }
}

impl fmt::Debug for dyn HashTree + '_ {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("HashTree").finish_non_exhaustive()
    }
}

/// No-op hasher that returns `H256::zero()` for all operations.
impl HashTree for () {
    fn name(&self) -> &'static str {
        "no_op256"
    }

    fn hash_leaf(&self, _value_hash: &ValueHash, _leaf_index: u64) -> ValueHash {
        ValueHash::zero()
    }

    fn hash_branch(&self, _lhs: &ValueHash, _rhs: &ValueHash) -> ValueHash {
        ValueHash::zero()
    }

    fn empty_subtree_hash(&self, _depth: usize) -> ValueHash {
        ValueHash::zero()
    }
}

impl HashTree for Blake2Hasher {
    fn name(&self) -> &'static str {
        "blake2s256"
    }

    fn hash_leaf(&self, value_hash: &ValueHash, leaf_index: u64) -> ValueHash {
        let mut bytes = [0_u8; 40];
        bytes[..8].copy_from_slice(&leaf_index.to_be_bytes());
        bytes[8..].copy_from_slice(value_hash.as_ref());
        self.hash_bytes(&bytes)
    }

    /// Compresses the hashes of 2 children in a branch node.
    fn hash_branch(&self, lhs: &ValueHash, rhs: &ValueHash) -> ValueHash {
        self.compress(lhs, rhs)
    }

    /// Returns the hash of an empty subtree with the given depth.
    fn empty_subtree_hash(&self, depth: usize) -> ValueHash {
        static EMPTY_TREE_HASHES: LazyLock<Vec<ValueHash>> =
            LazyLock::new(compute_empty_tree_hashes);
        EMPTY_TREE_HASHES[depth]
    }
}

fn compute_empty_tree_hashes() -> Vec<ValueHash> {
    let empty_leaf_hash = Blake2Hasher.hash_bytes(&[0_u8; 40]);
    iter::successors(Some(empty_leaf_hash), |hash| {
        Some(Blake2Hasher.hash_branch(hash, hash))
    })
    .take(TREE_DEPTH + 1)
    .collect()
}

/// Hasher that keeps track of hashing metrics.
///
/// On drop, the metrics are merged into `shared_stats` (if present). Such roundabout handling
/// is motivated by efficiency; if atomics were to be used to track metrics (e.g.,
/// via a wrapping `HashTree` implementation), this would tank performance because of contention.
#[derive(Debug)]
pub(crate) struct HasherWithStats<'a> {
    inner: &'a dyn HashTree,
    shared_metrics: Option<&'a HashingStats>,
    local_hashed_bytes: u64,
}

impl<'a> HasherWithStats<'a> {
    pub fn new(inner: &'a dyn HashTree) -> Self {
        Self {
            inner,
            shared_metrics: None,
            local_hashed_bytes: 0,
        }
    }
}

impl<'a> AsRef<(dyn HashTree + 'a)> for HasherWithStats<'a> {
    fn as_ref(&self) -> &(dyn HashTree + 'a) {
        self.inner
    }
}

impl Drop for HasherWithStats<'_> {
    fn drop(&mut self) {
        if let Some(shared_stats) = self.shared_metrics {
            shared_stats.add_hashed_bytes(self.local_hashed_bytes);
        }
    }
}

impl HasherWithStats<'_> {
    fn hash_leaf(&mut self, value_hash: &ValueHash, leaf_index: u64) -> ValueHash {
        const HASHED_BYTES: u64 = 8 + ValueHash::len_bytes() as u64;

        self.local_hashed_bytes += HASHED_BYTES;
        self.inner.hash_leaf(value_hash, leaf_index)
    }

    fn hash_branch(&mut self, lhs: &ValueHash, rhs: &ValueHash) -> ValueHash {
        const HASHED_BYTES: u64 = 2 * ValueHash::len_bytes() as u64;

        self.local_hashed_bytes += HASHED_BYTES;
        self.inner.hash_branch(lhs, rhs)
    }

    fn hash_optional_branch(
        &mut self,
        subtree_depth: usize,
        lhs: Option<ValueHash>,
        rhs: Option<ValueHash>,
    ) -> Option<ValueHash> {
        match (lhs, rhs) {
            (None, None) => None,
            (Some(lhs), None) => {
                let empty_hash = self.empty_subtree_hash(subtree_depth);
                Some(self.hash_branch(&lhs, &empty_hash))
            }
            (None, Some(rhs)) => {
                let empty_hash = self.empty_subtree_hash(subtree_depth);
                Some(self.hash_branch(&empty_hash, &rhs))
            }
            (Some(lhs), Some(rhs)) => Some(self.hash_branch(&lhs, &rhs)),
        }
    }

    pub fn empty_subtree_hash(&self, depth: usize) -> ValueHash {
        self.inner.empty_subtree_hash(depth)
    }
}

#[cfg(test)]
mod tests {
    use zksync_types::{AccountTreeId, Address, StorageKey, H256};

    use super::*;
    use crate::types::LeafNode;

    #[test]
    fn empty_tree_hash_is_as_expected() {
        const EXPECTED_HASH: ValueHash = H256([
            152, 164, 142, 78, 209, 115, 97, 136, 56, 74, 232, 167, 157, 210, 28, 77, 102, 135,
            229, 253, 34, 202, 24, 20, 137, 6, 215, 135, 54, 192, 216, 106,
        ]);

        let hasher: &dyn HashTree = &Blake2Hasher;
        assert_eq!(hasher.empty_tree_hash(), EXPECTED_HASH);
    }

    #[test]
    fn leaf_is_hashed_as_expected() {
        // Reference value taken from the previous implementation.
        const EXPECTED_HASH: ValueHash = H256([
            127, 0, 166, 178, 238, 222, 150, 8, 87, 112, 60, 140, 185, 233, 111, 40, 185, 16, 230,
            105, 52, 18, 206, 164, 176, 6, 242, 66, 57, 182, 129, 224,
        ]);

        let address: Address = "4b3af74f66ab1f0da3f2e4ec7a3cb99baf1af7b2".parse().unwrap();
        let key = StorageKey::new(AccountTreeId::new(address), H256::zero());
        let key = key.hashed_key_u256();
        let leaf = LeafNode::new(TreeEntry::new(key, 1, H256([1; 32])));

        let stats = HashingStats::default();
        let mut hasher = (&Blake2Hasher as &dyn HashTree).with_stats(&stats);
        let leaf_hash = leaf.hash(&mut hasher, 0);
        assert_eq!(leaf_hash, EXPECTED_HASH);

        drop(hasher);
        assert!(stats.hashed_bytes.into_inner() > 100);

        let hasher: &dyn HashTree = &Blake2Hasher;
        let folded_hash = hasher.fold_merkle_path(&[], leaf.into());
        assert_eq!(folded_hash, EXPECTED_HASH);
    }

    #[test]
    fn folding_merkle_path() {
        let address: Address = "4b3af74f66ab1f0da3f2e4ec7a3cb99baf1af7b2".parse().unwrap();
        let key = StorageKey::new(AccountTreeId::new(address), H256::zero());
        let key = key.hashed_key_u256();
        let leaf = LeafNode::new(TreeEntry::new(key, 1, H256([1; 32])));

        let mut hasher = HasherWithStats::new(&Blake2Hasher);
        let leaf_hash = leaf.hash(&mut hasher, 2);
        assert!(key.bit(254) && !key.bit(255));
        let merkle_path = [H256([2; 32]), H256([3; 32])];
        let expected_hash = hasher.hash_branch(&merkle_path[0], &leaf_hash);
        let expected_hash = hasher.hash_branch(&expected_hash, &merkle_path[1]);

        let folded_hash = hasher.inner.fold_merkle_path(&merkle_path, leaf.into());
        assert_eq!(folded_hash, expected_hash);
    }
}
