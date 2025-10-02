//! Consistency verification for the Merkle tree.

use std::sync::atomic::{AtomicU64, Ordering};

use rayon::prelude::*;

use crate::{
    errors::DeserializeError,
    hasher::{HashTree, HasherWithStats},
    types::{LeafNode, Nibbles, Node, NodeKey, Root},
    Database, Key, MerkleTree, ValueHash,
};

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ConsistencyError {
    #[error("failed deserializing node from DB: {0}")]
    Deserialize(#[from] DeserializeError),
    #[error("tree version {0} does not exist")]
    MissingVersion(u64),
    #[error("missing root for tree version {0}")]
    MissingRoot(u64),
    #[error(
        "missing {node_str} at {key}",
        node_str = if *is_leaf { "leaf" } else { "internal node" }
    )]
    MissingNode { key: NodeKey, is_leaf: bool },
    #[error("internal node at terminal tree level {key}")]
    TerminalInternalNode { key: NodeKey },
    #[error("tree root specifies that tree has {expected} leaves, but it actually has {actual}")]
    LeafCountMismatch { expected: u64, actual: u64 },
    #[error(
        "internal node at {key} specifies that child hash at `{nibble:x}` \
         is {expected}, but it actually is {actual}"
    )]
    HashMismatch {
        key: NodeKey,
        nibble: u8,
        expected: ValueHash,
        actual: ValueHash,
    },
    #[error(
        "leaf at {key} specifies its full key as {full_key}, which doesn't start with the node key"
    )]
    FullKeyMismatch { key: NodeKey, full_key: Key },
    #[error("leaf with key {full_key} has zero index, while leaf indices must start with 1")]
    ZeroIndex { full_key: Key },
    #[error(
        "leaf with key {full_key} has index {index}, which is greater than \
         leaf count {leaf_count} specified at tree root"
    )]
    LeafIndexOverflow {
        index: u64,
        leaf_count: u64,
        full_key: Key,
    },
    #[error("leaf with key {full_key} has same index {index} as another key")]
    DuplicateLeafIndex { index: u64, full_key: Key },
    #[error("internal node with key {key} does not have children")]
    EmptyInternalNode { key: NodeKey },
    #[error(
        "internal node with key {key} should have version {expected_version} (max among child ref versions)"
    )]
    KeyVersionMismatch { key: NodeKey, expected_version: u64 },
    #[error("root node should have version >={max_child_version} (max among child ref versions)")]
    RootVersionMismatch { max_child_version: u64 },
}

impl<DB: Database, H: HashTree> MerkleTree<DB, H> {
    /// Verifies the internal tree consistency as stored in the database.
    ///
    /// If `validate_indices` flag is set, it will be checked that indices for all tree leaves are unique
    /// and are sequentially assigned starting from 1.
    ///
    /// # Errors
    ///
    /// Returns an error (the first encountered one if there are multiple).
    pub fn verify_consistency(
        &self,
        version: u64,
        validate_indices: bool,
    ) -> Result<(), ConsistencyError> {
        let manifest = self.db.try_manifest()?;
        let manifest = manifest.ok_or(ConsistencyError::MissingVersion(version))?;
        if version >= manifest.version_count {
            return Err(ConsistencyError::MissingVersion(version));
        }

        let root = self
            .db
            .try_root(version)?
            .ok_or(ConsistencyError::MissingRoot(version))?;
        let (leaf_count, root_node) = match root {
            Root::Empty => return Ok(()),
            Root::Filled { leaf_count, node } => (leaf_count.get(), node),
        };

        // We want to perform a depth-first walk of the tree in order to not keep
        // much in memory.
        let root_key = Nibbles::EMPTY.with_version(version);
        let leaf_data = validate_indices.then(|| LeafConsistencyData::new(leaf_count));
        self.validate_node(&root_node, root_key, leaf_data.as_ref())?;
        if let Some(leaf_data) = leaf_data {
            leaf_data.validate_count()?;
        }
        Ok(())
    }

    fn validate_node(
        &self,
        node: &Node,
        key: NodeKey,
        leaf_data: Option<&LeafConsistencyData>,
    ) -> Result<ValueHash, ConsistencyError> {
        match node {
            Node::Leaf(leaf) => {
                let full_key_nibbles = Nibbles::new(&leaf.full_key, key.nibbles.nibble_count());
                if full_key_nibbles != key.nibbles {
                    return Err(ConsistencyError::FullKeyMismatch {
                        key,
                        full_key: leaf.full_key,
                    });
                }
                if let Some(leaf_data) = leaf_data {
                    leaf_data.insert_leaf(leaf)?;
                }
            }

            Node::Internal(node) => {
                let expected_version = node.child_refs().map(|child_ref| child_ref.version).max();
                let Some(expected_version) = expected_version else {
                    return Err(ConsistencyError::EmptyInternalNode { key });
                };
                if !key.is_empty() && expected_version != key.version {
                    return Err(ConsistencyError::KeyVersionMismatch {
                        key,
                        expected_version,
                    });
                } else if key.is_empty() && expected_version > key.version {
                    return Err(ConsistencyError::RootVersionMismatch {
                        max_child_version: expected_version,
                    });
                }

                // `.into_par_iter()` below is the only place where `rayon`-based parallelism
                // is used in tree verification.
                let children: Vec<_> = node.children().collect();
                children
                    .into_par_iter()
                    .try_for_each(|(nibble, child_ref)| {
                        let child_key = key
                            .nibbles
                            .push(nibble)
                            .ok_or(ConsistencyError::TerminalInternalNode { key })?;
                        let child_key = child_key.with_version(child_ref.version);
                        let child = self
                            .db
                            .try_tree_node(&child_key, child_ref.is_leaf)?
                            .ok_or(ConsistencyError::MissingNode {
                                key: child_key,
                                is_leaf: child_ref.is_leaf,
                            })?;

                        // Recursion here is OK; the tree isn't that deep (approximately 8 nibbles for a tree with
                        // approximately 1B entries).
                        let child_hash = self.validate_node(&child, child_key, leaf_data)?;
                        if child_hash == child_ref.hash {
                            Ok(())
                        } else {
                            Err(ConsistencyError::HashMismatch {
                                key,
                                nibble,
                                expected: child_ref.hash,
                                actual: child_hash,
                            })
                        }
                    })?;
            }
        }

        let level = key.nibbles.nibble_count() * 4;
        Ok(node.hash(&mut HasherWithStats::new(&self.hasher), level))
    }
}

#[derive(Debug)]
struct LeafConsistencyData {
    expected_leaf_count: u64,
    actual_leaf_count: AtomicU64,
    leaf_indices_set: AtomicBitSet,
}

#[allow(clippy::cast_possible_truncation)] // expected leaf count is quite small
impl LeafConsistencyData {
    fn new(expected_leaf_count: u64) -> Self {
        Self {
            expected_leaf_count,
            actual_leaf_count: AtomicU64::new(0),
            leaf_indices_set: AtomicBitSet::new(expected_leaf_count as usize),
        }
    }

    fn insert_leaf(&self, leaf: &LeafNode) -> Result<(), ConsistencyError> {
        if leaf.leaf_index == 0 {
            return Err(ConsistencyError::ZeroIndex {
                full_key: leaf.full_key,
            });
        }
        if leaf.leaf_index > self.expected_leaf_count {
            return Err(ConsistencyError::LeafIndexOverflow {
                index: leaf.leaf_index,
                leaf_count: self.expected_leaf_count,
                full_key: leaf.full_key,
            });
        }

        let index = (leaf.leaf_index - 1) as usize;
        if self.leaf_indices_set.set(index) {
            return Err(ConsistencyError::DuplicateLeafIndex {
                index: leaf.leaf_index,
                full_key: leaf.full_key,
            });
        }
        self.actual_leaf_count.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    fn validate_count(mut self) -> Result<(), ConsistencyError> {
        let actual_leaf_count = *self.actual_leaf_count.get_mut();
        if actual_leaf_count == self.expected_leaf_count {
            Ok(())
        } else {
            Err(ConsistencyError::LeafCountMismatch {
                expected: self.expected_leaf_count,
                actual: actual_leaf_count,
            })
        }
    }
}

/// Primitive atomic bit set implementation that only supports setting bits.
#[derive(Debug)]
struct AtomicBitSet {
    bits: Vec<AtomicU64>,
}

impl AtomicBitSet {
    const BITS_PER_ATOMIC: usize = 64;

    fn new(len: usize) -> Self {
        let atomic_count = len.div_ceil(Self::BITS_PER_ATOMIC);
        let mut bits = Vec::with_capacity(atomic_count);
        bits.resize_with(atomic_count, AtomicU64::default);
        Self { bits }
    }

    /// Returns the previous bit value.
    fn set(&self, bit_index: usize) -> bool {
        let atomic_index = bit_index / Self::BITS_PER_ATOMIC;
        let shift_in_atomic = bit_index % Self::BITS_PER_ATOMIC;
        let atomic = &self.bits[atomic_index];
        let mask = 1 << (shift_in_atomic as u64);
        let prev_value = atomic.fetch_or(mask, Ordering::SeqCst);
        prev_value & mask != 0
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;

    use assert_matches::assert_matches;
    use rayon::ThreadPoolBuilder;
    use zksync_types::{H256, U256};

    use super::*;
    use crate::{
        types::{InternalNode, TreeEntry},
        PatchSet,
    };

    const FIRST_KEY: Key = U256([0, 0, 0, 0x_dead_beef_0000_0000]);
    const SECOND_KEY: Key = U256([0, 0, 0, 0x_dead_beef_0100_0000]);

    fn prepare_database() -> PatchSet {
        let mut tree = MerkleTree::new(PatchSet::default()).unwrap();
        tree.extend(vec![
            TreeEntry::new(FIRST_KEY, 1, H256([1; 32])),
            TreeEntry::new(SECOND_KEY, 2, H256([2; 32])),
        ])
        .unwrap();
        tree.db
    }

    #[test]
    fn atomic_bit_set_basics() {
        let bit_set = AtomicBitSet::new(10);
        assert!(!bit_set.set(3));
        assert!(!bit_set.set(7));
        assert!(!bit_set.set(6));
        assert!(!bit_set.set(9));
        assert!(bit_set.set(3));
        assert!(bit_set.set(7));
        assert!(!bit_set.set(0));
    }

    #[test]
    fn basic_consistency_checks() {
        let db = prepare_database();
        deterministic_verify_consistency(db).unwrap();
    }

    /// Limits the number of `rayon` threads to 1 in order to get deterministic test execution.
    fn deterministic_verify_consistency(db: PatchSet) -> Result<(), ConsistencyError> {
        let thread_pool = ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .expect("failed initializing `rayon` thread pool");
        thread_pool.install(|| MerkleTree::new(db).unwrap().verify_consistency(0, true))
    }

    #[test]
    fn missing_version_error() {
        let mut db = prepare_database();
        db.manifest_mut().version_count = 0;

        let err = deterministic_verify_consistency(db).unwrap_err();
        assert_matches!(err, ConsistencyError::MissingVersion(0));
    }

    #[test]
    fn missing_root_error() {
        let mut db = prepare_database();
        db.remove_root(0);

        let err = deterministic_verify_consistency(db).unwrap_err();
        assert_matches!(err, ConsistencyError::MissingRoot(0));
    }

    #[test]
    fn missing_node_error() {
        let mut db = prepare_database();

        let leaf_key = db
            .nodes_mut()
            .find_map(|(key, node)| matches!(node, Node::Leaf(_)).then(|| *key));
        let leaf_key = leaf_key.unwrap();
        db.remove_node(&leaf_key);

        let err = deterministic_verify_consistency(db).unwrap_err();
        assert_matches!(
            err,
            ConsistencyError::MissingNode { key, is_leaf: true } if key == leaf_key
        );
    }

    #[test]
    fn leaf_count_mismatch_error() {
        let mut db = prepare_database();

        let root = db.root_mut(0).unwrap();
        let Root::Filled { leaf_count, .. } = root else {
            panic!("unexpected root: {root:?}");
        };
        *leaf_count = NonZeroU64::new(42).unwrap();

        let err = deterministic_verify_consistency(db).unwrap_err();
        assert_matches!(
            err,
            ConsistencyError::LeafCountMismatch {
                expected: 42,
                actual: 2
            }
        );
    }

    #[test]
    fn hash_mismatch_error() {
        let mut db = prepare_database();

        let root = db.root_mut(0).unwrap();
        let Root::Filled {
            node: Node::Internal(node),
            ..
        } = root
        else {
            panic!("unexpected root: {root:?}");
        };
        let child_ref = node.child_ref_mut(0xd).unwrap();
        child_ref.hash = ValueHash::zero();

        let err = deterministic_verify_consistency(db).unwrap_err();
        assert_matches!(
            err,
            ConsistencyError::HashMismatch {
                key,
                nibble: 0xd,
                expected,
                ..
            } if key == NodeKey::empty(0) && expected == ValueHash::zero()
        );
    }

    #[test]
    fn full_key_mismatch_error() {
        let mut db = prepare_database();

        let leaf_key = db.nodes_mut().find_map(|(key, node)| {
            if let Node::Leaf(leaf) = node {
                leaf.full_key = U256::zero();
                return Some(*key);
            }
            None
        });
        let leaf_key = leaf_key.unwrap();

        let err = deterministic_verify_consistency(db).unwrap_err();
        assert_matches!(
            err,
            ConsistencyError::FullKeyMismatch { key, full_key }
                if key == leaf_key && full_key == U256::zero()
        );
    }

    #[test]
    fn leaf_index_overflow_error() {
        let mut db = prepare_database();

        let leaf_key = db.nodes_mut().find_map(|(key, node)| {
            if let Node::Leaf(leaf) = node {
                leaf.leaf_index = 42;
                return Some(*key);
            }
            None
        });
        leaf_key.unwrap();

        let err = deterministic_verify_consistency(db).unwrap_err();
        assert_matches!(
            err,
            ConsistencyError::LeafIndexOverflow {
                index: 42,
                leaf_count: 2,
                ..
            }
        );
    }

    #[test]
    fn duplicate_leaf_index_error() {
        let mut db = prepare_database();

        for (_, node) in db.nodes_mut() {
            if let Node::Leaf(leaf) = node {
                leaf.leaf_index = 1;
            }
        }

        let err = deterministic_verify_consistency(db).unwrap_err();
        assert_matches!(err, ConsistencyError::DuplicateLeafIndex { index: 1, .. });
    }

    #[test]
    fn empty_internal_node_error() {
        let mut db = prepare_database();
        let node_key = db.nodes_mut().find_map(|(key, node)| {
            if let Node::Internal(node) = node {
                *node = InternalNode::default();
                return Some(*key);
            }
            None
        });
        let node_key = node_key.unwrap();

        let err = deterministic_verify_consistency(db).unwrap_err();
        assert_matches!(err, ConsistencyError::EmptyInternalNode { key } if key == node_key);
    }

    #[test]
    fn version_mismatch_error() {
        let mut db = prepare_database();
        let node_key = db.nodes_mut().find_map(|(key, node)| {
            if let Node::Internal(node) = node {
                let (nibble, _) = node.children().next().unwrap();
                node.child_ref_mut(nibble).unwrap().version = 1;
                return Some(*key);
            }
            None
        });
        let node_key = node_key.unwrap();

        let err = deterministic_verify_consistency(db).unwrap_err();
        assert_matches!(
            err,
            ConsistencyError::KeyVersionMismatch { key, expected_version: 1 } if key == node_key
        );
    }

    #[test]
    fn root_version_mismatch_error() {
        let mut db = prepare_database();
        let Some(Root::Filled {
            node: Node::Internal(node),
            ..
        }) = db.root_mut(0)
        else {
            unreachable!();
        };
        let (nibble, _) = node.children().next().unwrap();
        node.child_ref_mut(nibble).unwrap().version = 42;

        let err = deterministic_verify_consistency(db).unwrap_err();
        assert_matches!(
            err,
            ConsistencyError::RootVersionMismatch {
                max_child_version: 42,
            }
        );
    }
}
